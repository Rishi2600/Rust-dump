use crate::error::SchedError;
use crate::executor::Status;
use crate::ledger::Ledger;
use crate::node::{Block, Node, NodeConfig};
use crate::priority::Policy;
use crate::scenario::Scenario;
use crate::tx::Transaction;

/// A transaction the pool would not take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub slot: u64,
    pub tx: Transaction,
    pub reason: SchedError,
}

/// Everything that happened when a scenario was run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub blocks: Vec<Block>,
    pub rejections: Vec<Rejection>,
    /// The ledger after the last block.
    pub ledger: Ledger,
}

impl Run {
    /// The fees charged over the whole run.
    pub fn fees(&self) -> u64 {
        self.blocks.iter().map(|block| block.fees).sum()
    }

    /// How many transactions made it into a block.
    pub fn included(&self) -> usize {
        self.blocks.iter().map(|block| block.receipts.len()).sum()
    }

    /// How many of those were charged but did not succeed.
    pub fn failed(&self) -> usize {
        let receipts = self.blocks.iter().flat_map(|block| &block.receipts);
        receipts
            .filter(|receipt| matches!(receipt.status, Status::Failed(_)))
            .count()
    }

    /// How many waited too long and were dropped.
    pub fn expired(&self) -> usize {
        self.blocks.iter().map(|block| block.expired.len()).sum()
    }
}

/// Feed a scenario to a node, slot by slot, until every transaction has
/// either been included or dropped.
pub fn run_scenario(scenario: &Scenario, config: &NodeConfig) -> Run {
    let mut node = Node::new(scenario.ledger(), config.clone());
    let (mut blocks, mut rejections) = (Vec::new(), Vec::new());
    let last_arrival = scenario.last_slot();
    // Once nothing more arrives, the pool can only shrink, and anything
    // that cannot run expires. The bound is a guard against a time limit
    // set so high that waiting it out is pointless.
    let patience = config.pool.ttl_slots.min(1_000) + 1;
    let mut quiet = 0;
    while quiet <= patience {
        let slot = node.slot();
        for tx in scenario.arrivals.get(&slot).into_iter().flatten() {
            if let Err(reason) = node.submit(tx.clone()) {
                let tx = tx.clone();
                rejections.push(Rejection { slot, tx, reason });
            }
        }
        let block = node.produce_block();
        let idle = block.schedule.is_empty() && block.expired.is_empty();
        quiet = if idle { quiet + 1 } else { 0 };
        blocks.push(block);
        if slot >= last_arrival && node.pool().is_empty() {
            break;
        }
    }
    // Slots at the end in which nothing happened are not worth reporting.
    while blocks.len() > 1
        && blocks
            .last()
            .is_some_and(|block| block.schedule.is_empty() && block.expired.is_empty())
    {
        blocks.pop();
    }
    Run {
        blocks,
        rejections,
        ledger: node.ledger().clone(),
    }
}

/// Run the same scenario once under each policy.
pub fn compare_policies(scenario: &Scenario, config: &NodeConfig) -> Vec<(Policy, Run)> {
    let run_under = |policy: Policy| {
        let mut config = config.clone();
        config.pool.policy = policy;
        (policy, run_scenario(scenario, &config))
    };
    Policy::ALL.into_iter().map(run_under).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nine unrelated senders compete for blocks that hold 300 units, and
    /// nothing may wait longer than two slots.
    const CROWDED: &str = "\
limit units 300
limit ttl 2
account s1 1000
account s2 1000
account s3 1000
account s4 1000
account s5 1000
account s6 1000
account s7 1000
account s8 1000
account s9 1000
tx s1 pays p1 1 fee 5
tx s2 pays p2 1 fee 10
tx s3 pays p3 1 fee 150 units 300
tx s4 pays p4 1 fee 15
tx s5 pays p5 1 fee 90
tx s6 pays p6 1 fee 140 units 300
tx s7 pays p7 1 fee 120
tx s8 pays p8 1 fee 8
tx s9 pays p9 1 fee 110
";

    fn crowded() -> (Scenario, NodeConfig) {
        let scenario = Scenario::parse(CROWDED).unwrap();
        let mut config = NodeConfig::default();
        scenario.configure(&mut config);
        (scenario, config)
    }

    fn ids(block: &Block) -> Vec<u64> {
        block.schedule.ids()
    }

    #[test]
    fn a_scenario_runs_until_everything_is_included_or_dropped() {
        let (scenario, config) = crowded();
        let run = run_scenario(&scenario, &config);
        // Three small high-rate transactions fill the first block, the best
        // large one fills the second, and the rest run out of time.
        assert_eq!(ids(&run.blocks[0]), [7, 9, 5]);
        assert_eq!(ids(&run.blocks[1]), [3]);
        assert_eq!(run.blocks[2].expired, [1, 2, 4, 6, 8]);
        assert_eq!(run.blocks.len(), 3);
        assert_eq!((run.included(), run.expired(), run.failed()), (4, 5, 0));
        assert_eq!(run.fees(), 120 + 110 + 90 + 150);
        assert_eq!(run.ledger.balance("validator"), run.fees());
        assert!(run.rejections.is_empty());
    }

    #[test]
    fn ranking_by_fee_rate_collects_the_most_when_space_is_short() {
        let (scenario, config) = crowded();
        let results = compare_policies(&scenario, &config);
        let fees: Vec<(Policy, u64)> = results
            .iter()
            .map(|(policy, run)| (*policy, run.fees()))
            .collect();
        // By rate: 120 + 110 + 90, then 150.
        // By total fee: the two 300-unit transactions take a block each.
        // By arrival: the first three that fit pay 5, 10 and 15.
        assert_eq!(
            fees,
            [
                (Policy::FeePerUnit, 470),
                (Policy::TotalFee, 290),
                (Policy::Fifo, 180)
            ]
        );
    }

    #[test]
    fn with_room_for_everyone_the_policies_collect_the_same() {
        let (scenario, mut config) = crowded();
        config.limits.max_units = 100_000;
        let results = compare_policies(&scenario, &config);
        for (policy, run) in &results {
            assert_eq!(
                (run.fees(), run.included(), run.blocks.len()),
                (648, 9, 1),
                "{policy}"
            );
        }
    }

    #[test]
    fn transactions_the_pool_refuses_are_recorded_with_the_reason() {
        let text = "account alice 100\ntx alice pays bob 1 fee 5\ntx nobody pays bob 1 fee 5\n";
        let run = run_scenario(&Scenario::parse(text).unwrap(), &NodeConfig::default());
        assert_eq!(run.included(), 1);
        assert_eq!(run.rejections.len(), 1);
        let rejection = &run.rejections[0];
        assert_eq!((rejection.slot, rejection.tx.id), (0, 2));
        assert!(matches!(rejection.reason, SchedError::CannotPayFee { .. }));
    }

    #[test]
    fn transactions_arrive_in_the_slot_the_scenario_gives_them() {
        let text =
            "account alice 100\ntx alice pays bob 1 fee 5\nslot 3\ntx alice pays bob 1 fee 5\n";
        let run = run_scenario(&Scenario::parse(text).unwrap(), &NodeConfig::default());
        let included: Vec<usize> = run
            .blocks
            .iter()
            .map(|block| block.receipts.len())
            .collect();
        assert_eq!(included, [1, 0, 0, 1]);
    }

    #[test]
    fn an_empty_scenario_produces_one_empty_block() {
        let run = run_scenario(&Scenario::default(), &NodeConfig::default());
        assert_eq!((run.blocks.len(), run.included(), run.fees()), (1, 0, 0));
    }

    #[test]
    fn a_stuck_transaction_does_not_keep_the_run_going_forever() {
        // Nonce 0 is refused for its fee, so nonce 1 can never run.
        let text = "limit ttl 4\naccount alice 100\ntx alice pays bob 1 fee 500\ntx alice pays bob 1 fee 5\n";
        let scenario = Scenario::parse(text).unwrap();
        let mut config = NodeConfig::default();
        scenario.configure(&mut config);
        let run = run_scenario(&scenario, &config);
        assert_eq!(
            (run.included(), run.expired(), run.rejections.len()),
            (0, 1, 1)
        );
        assert_eq!(run.blocks.len(), 5);
    }
}
