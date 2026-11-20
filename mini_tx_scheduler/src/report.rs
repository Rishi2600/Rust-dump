//! Turning results into text a person can read.

use std::fmt::Write as _;

use crate::executor::{Receipt, Status};
use crate::node::Block;
use crate::priority::Policy;
use crate::sim::Run;
use crate::tx::Transaction;

/// One transaction in a few words: `alice -> bob 100, carol 5 (reads oracle)`.
pub fn describe_tx(tx: &Transaction) -> String {
    let payments: Vec<String> = tx
        .transfers
        .iter()
        .map(|transfer| format!("{} {}", transfer.to, transfer.amount))
        .collect();
    let mut text = format!("{} -> {}", tx.sender, payments.join(", "));
    if !tx.reads.is_empty() {
        let _ = write!(text, " (reads {})", tx.reads.join(", "));
    }
    text
}

fn outcome(receipt: Option<&Receipt>) -> String {
    match receipt.map(|receipt| &receipt.status) {
        Some(Status::Success) => "ok".to_string(),
        Some(Status::Failed(reason)) => format!("FAILED: {reason}"),
        Some(Status::Rejected(reason)) => format!("REJECTED: {reason}"),
        None => "?".to_string(),
    }
}

/// A block, batch by batch, one line per transaction.
pub fn describe_block(block: &Block) -> String {
    let schedule = &block.schedule;
    let batches = schedule.batches();
    let mut text = format!(
        "slot {}: {} transactions in {} batches, {} units, fees {}\n",
        block.slot,
        schedule.len(),
        batches.len(),
        schedule.units,
        block.fees
    );
    for (index, batch) in batches.iter().enumerate() {
        let _ = writeln!(text, "  batch {index}");
        for tx in batch {
            let receipt = block.receipts.iter().find(|receipt| receipt.id == tx.id);
            let rate = tx.fee as f64 / tx.units as f64;
            let _ = writeln!(
                text,
                "    #{:<3} {:<34} fee {:>4}  units {:>4}  {:>5.2}/unit  {}",
                tx.id,
                describe_tx(tx),
                tx.fee,
                tx.units,
                rate,
                outcome(receipt)
            );
        }
    }
    if !block.expired.is_empty() {
        let ids: Vec<String> = block.expired.iter().map(|id| format!("#{id}")).collect();
        let _ = writeln!(text, "  expired after waiting too long: {}", ids.join(" "));
    }
    text
}

/// The totals of a run on one line.
pub fn describe_totals(run: &Run) -> String {
    format!(
        "{} included ({} failed), {} expired, {} refused, {} blocks, fees {}",
        run.included(),
        run.failed(),
        run.expired(),
        run.rejections.len(),
        run.blocks.len(),
        run.fees()
    )
}

/// A whole run: every block, what was refused, and the final balances.
pub fn describe_run(run: &Run) -> String {
    let mut text = String::new();
    for block in &run.blocks {
        text.push_str(&describe_block(block));
    }
    if !run.rejections.is_empty() {
        text.push_str("\nrefused by the pool\n");
        for rejection in &run.rejections {
            let tx = &rejection.tx;
            let _ = writeln!(
                text,
                "  slot {}  #{} {}: {}",
                rejection.slot,
                tx.id,
                describe_tx(tx),
                rejection.reason
            );
        }
    }
    text.push_str("\nfinal balances\n");
    for (account, balance) in run.ledger.accounts() {
        let _ = writeln!(text, "  {account:<12} {balance:>8}");
    }
    let _ = writeln!(text, "\ntotals: {}", describe_totals(run));
    text
}

/// A table of how the same scenario went under each policy.
pub fn describe_comparison(results: &[(Policy, Run)]) -> String {
    let blocks = results
        .iter()
        .map(|(_, run)| run.blocks.len())
        .max()
        .unwrap_or(0);
    let mut text = format!("{:<14}", "policy");
    for index in 0..blocks {
        let _ = write!(text, "{:>9}", format!("slot {index}"));
    }
    let _ = writeln!(text, "{:>9}{:>10}{:>9}", "fees", "included", "expired");
    for (policy, run) in results {
        let _ = write!(text, "{:<14}", policy.name());
        for index in 0..blocks {
            let fees = run.blocks.get(index).map_or(0, |block| block.fees);
            let _ = write!(text, "{fees:>9}");
        }
        let _ = writeln!(
            text,
            "{:>9}{:>10}{:>9}",
            run.fees(),
            run.included(),
            run.expired()
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeConfig;
    use crate::scenario::Scenario;
    use crate::sim::{compare_policies, run_scenario};

    const SMALL: &str = "\
limit ttl 2
account alice 1000
account bob 1000
account dave 50
tx alice pays shop 100 fee 10
tx bob pays shop 200 and carol 5 fee 80 units 200 reads oracle
tx dave pays shop 900 fee 20
tx nobody pays shop 1 fee 1
tx alice pays shop 1 fee 5
tx alice pays shop 1 fee 5
slot 1
tx carol pays shop 1 fee 1
";

    fn small_run() -> Run {
        let scenario = Scenario::parse(SMALL).unwrap();
        let mut config = NodeConfig::default();
        scenario.configure(&mut config);
        run_scenario(&scenario, &config)
    }

    #[test]
    fn a_transaction_is_described_by_who_pays_whom() {
        let tx = Transaction::new(1, "alice", 0)
            .pay("bob", 100)
            .pay("carol", 5);
        assert_eq!(describe_tx(&tx), "alice -> bob 100, carol 5");
        assert_eq!(
            describe_tx(&tx.reading("oracle").reading("prices")),
            "alice -> bob 100, carol 5 (reads oracle, prices)"
        );
    }

    #[test]
    fn a_block_is_listed_batch_by_batch() {
        let run = small_run();
        let text = describe_block(&run.blocks[0]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "slot 0: 5 transactions in 5 batches, 600 units, fees 120"
        );
        assert_eq!(lines[1], "  batch 0");
        // Bob pays the best rate: 80 for 200 units.
        assert!(
            lines[2].starts_with("    #2   bob -> shop 200, carol 5 (reads oracle)"),
            "{}",
            lines[2]
        );
        assert!(
            lines[2].ends_with("fee   80  units  200   0.40/unit  ok"),
            "{}",
            lines[2]
        );
        // Dave cannot afford his payment. He is charged and marked.
        let dave = lines
            .iter()
            .find(|line| line.contains("dave -> shop 900"))
            .unwrap();
        assert!(dave.ends_with("FAILED: insufficient funds"), "{dave}");
    }

    #[test]
    fn a_run_report_covers_blocks_refusals_balances_and_totals() {
        let text = describe_run(&small_run());
        assert!(text.contains("slot 0: 5 transactions"));
        assert!(text.contains("slot 1: 1 transactions in 1 batches"));
        assert!(text.contains("refused by the pool\n  slot 0  #4 nobody -> shop 1: nobody has 0, not enough for a fee of 1"));
        assert!(text.contains("final balances\n"));
        assert!(text.contains("  validator         121\n"));
        assert!(text.ends_with(
            "totals: 6 included (1 failed), 0 expired, 1 refused, 2 blocks, fees 121\n"
        ));
    }

    #[test]
    fn expired_transactions_are_named_in_the_block_that_dropped_them() {
        let text = "limit ttl 2\naccount a 100\ntx a pays b 1 fee 500\ntx a pays b 1 fee 5\n";
        let run = run_scenario(&Scenario::parse(text).unwrap(), &{
            let mut config = NodeConfig::default();
            config.pool.ttl_slots = 2;
            config
        });
        let report = describe_run(&run);
        assert!(
            report.contains("  expired after waiting too long: #2\n"),
            "{report}"
        );
    }

    #[test]
    fn a_comparison_lines_the_policies_up_side_by_side() {
        let scenario = Scenario::parse(
            "limit units 100\nlimit ttl 1\naccount a 100\naccount b 100\ntx a pays x 1 fee 5\ntx b pays y 1 fee 50\n",
        )
        .unwrap();
        let mut config = NodeConfig::default();
        scenario.configure(&mut config);
        let text = describe_comparison(&compare_policies(&scenario, &config));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "policy           slot 0   slot 1     fees  included  expired"
        );
        assert_eq!(
            lines[1],
            "fee-per-unit         50        0       50         1        1"
        );
        assert_eq!(
            lines[2],
            "total-fee            50        0       50         1        1"
        );
        assert_eq!(
            lines[3],
            "fifo                  5        0        5         1        1"
        );
    }
}
