use crate::error::SchedResult;
use crate::executor::{Receipt, execute_batches};
use crate::ledger::Ledger;
use crate::mempool::{Admission, Mempool, PoolConfig};
use crate::scheduler::{BlockLimits, Schedule, build_schedule};
use crate::tx::Transaction;

/// How a node is set up.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub pool: PoolConfig,
    pub limits: BlockLimits,
    /// The account that receives the fees of every block.
    pub validator: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            pool: PoolConfig::default(),
            limits: BlockLimits::default(),
            validator: "validator".to_string(),
        }
    }
}

/// What one slot produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub slot: u64,
    /// What was chosen, and how it was arranged into batches.
    pub schedule: Schedule,
    /// How each chosen transaction turned out, batch by batch.
    pub receipts: Vec<Receipt>,
    /// The fees charged, in total.
    pub fees: u64,
    /// Ids of transactions dropped at the start of the slot for having
    /// waited too long.
    pub expired: Vec<u64>,
}

/// The whole machine: a ledger, a pool of waiting transactions, and a clock
/// that ticks in slots. Each slot it builds one block.
pub struct Node {
    ledger: Ledger,
    pool: Mempool,
    config: NodeConfig,
    slot: u64,
}

impl Node {
    pub fn new(ledger: Ledger, config: NodeConfig) -> Self {
        Self {
            ledger,
            pool: Mempool::new(config.pool.clone()),
            config,
            slot: 0,
        }
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub fn pool(&self) -> &Mempool {
        &self.pool
    }

    /// The slot the next block will be built in.
    pub fn slot(&self) -> u64 {
        self.slot
    }

    /// Offer a transaction to the node's pool.
    pub fn submit(&mut self, tx: Transaction) -> SchedResult<Admission> {
        self.pool.add(tx, &self.ledger, self.slot)
    }

    /// Build the block for this slot and move to the next slot.
    ///
    /// The steps are the life of a transaction: chosen from the pool by
    /// priority, arranged so that clashing ones do not overlap, executed,
    /// and finally removed from the pool.
    pub fn produce_block(&mut self) -> Block {
        let slot = self.slot;
        // Housekeeping first, so that nothing stale is considered.
        let expired = self.pool.expire(slot).iter().map(|tx| tx.id).collect();
        let policy = self.config.pool.policy;
        let schedule = build_schedule(&self.pool, &self.ledger, policy, &self.config.limits);
        let threads = self.config.limits.threads;
        let receipts = execute_batches(&mut self.ledger, &schedule.batches(), threads);
        for pick in &schedule.picks {
            self.pool.remove(&pick.tx.sender, pick.tx.nonce);
        }
        let fees = receipts.iter().map(|receipt| receipt.fee).sum();
        // Fees are paid out once, after the block, in a single credit. If
        // every transaction paid the validator directly, every transaction
        // would write the same account, each would clash with all the
        // others, and nothing could ever run in parallel.
        let _ = self.ledger.credit(&self.config.validator, fees);
        self.slot += 1;
        Block {
            slot,
            schedule,
            receipts,
            fees,
            expired,
        }
    }
}
