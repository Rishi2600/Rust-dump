use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use crate::error::{SchedError, SchedResult};
use crate::ledger::Ledger;
use crate::node::NodeConfig;
use crate::tx::{Transaction, is_name};
use crate::workload::Workload;

/// Limits a scenario file asks for. Anything left unset keeps its default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Settings {
    pub block_units: Option<u64>,
    pub block_txs: Option<usize>,
    pub threads: Option<usize>,
    pub ttl: Option<u64>,
}

/// A written-down situation to run: who has what, and which transactions
/// arrive when.
///
/// ```text
/// # comments start with a hash
/// account alice 1000
/// account bob 500
/// limit units 300
///
/// slot 0
/// tx alice pays bob 100 fee 10
/// tx bob pays carol 20 and dave 5 fee 40 units 200 reads oracle
///
/// slot 2
/// tx alice pays carol 1 fee 3
/// ```
///
/// Transactions are numbered from 1 in the order they appear, and each
/// sender's nonces count up from 0, so neither has to be written out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scenario {
    /// Starting balances, in the order declared.
    pub accounts: Vec<(String, u64)>,
    /// The transactions that arrive just before each slot's block is built.
    pub arrivals: BTreeMap<u64, Vec<Transaction>>,
    pub settings: Settings,
}

fn number<T: FromStr>(text: &str, what: &str) -> Result<T, String> {
    text.parse().map_err(|_| format!("`{text}` is not {what}"))
}

impl Scenario {
    pub fn parse(text: &str) -> SchedResult<Self> {
        let mut scenario = Scenario::default();
        let mut slot = 0u64;
        let mut nonces: BTreeMap<String, u64> = BTreeMap::new();
        for (index, raw) in text.lines().enumerate() {
            let fail = |msg: String| SchedError::Parse {
                line: index + 1,
                msg,
            };
            let line = raw.split('#').next().unwrap_or("").trim();
            let mut words = line.split_whitespace();
            let Some(keyword) = words.next() else {
                continue;
            };
            let rest: Vec<&str> = words.collect();
            match (keyword, rest.as_slice()) {
                ("account", [name, balance]) => {
                    if !is_name(name) {
                        return Err(fail(format!("`{name}` is not an account name")));
                    }
                    if scenario.accounts.iter().any(|(known, _)| known == name) {
                        return Err(fail(format!("account `{name}` is declared twice")));
                    }
                    let balance = number(balance, "a balance").map_err(&fail)?;
                    scenario.accounts.push((name.to_string(), balance));
                }
                ("slot", [value]) => {
                    let next = number(value, "a slot number").map_err(&fail)?;
                    if next < slot {
                        return Err(fail("slots must not go backwards".into()));
                    }
                    slot = next;
                }
                ("limit", [what, value]) => {
                    let settings = &mut scenario.settings;
                    match *what {
                        "units" => {
                            settings.block_units =
                                Some(number(value, "a number of units").map_err(&fail)?)
                        }
                        "txs" => {
                            settings.block_txs = Some(number(value, "a count").map_err(&fail)?)
                        }
                        "threads" => {
                            settings.threads = Some(number(value, "a count").map_err(&fail)?)
                        }
                        "ttl" => {
                            settings.ttl = Some(number(value, "a number of slots").map_err(&fail)?)
                        }
                        other => return Err(fail(format!("there is no limit called `{other}`"))),
                    }
                }
                ("tx", words) => {
                    let id = scenario.transaction_count() as u64 + 1;
                    let tx = parse_tx(words, id, &mut nonces).map_err(&fail)?;
                    tx.validate().map_err(|error| fail(error.to_string()))?;
                    scenario.arrivals.entry(slot).or_default().push(tx);
                }
                ("account", _) => return Err(fail("expected `account <name> <balance>`".into())),
                ("slot", _) => return Err(fail("expected `slot <number>`".into())),
                ("limit", _) => {
                    return Err(fail(
                        "expected `limit <units|txs|threads|ttl> <number>`".into(),
                    ));
                }
                (other, _) => {
                    return Err(fail(format!(
                        "`{other}` is not something a scenario can say"
                    )));
                }
            }
        }
        Ok(scenario)
    }

    /// Read and parse a scenario file.
    pub fn load(path: impl AsRef<Path>) -> SchedResult<Self> {
        let text =
            std::fs::read_to_string(path).map_err(|error| SchedError::Io(error.to_string()))?;
        Self::parse(&text)
    }

    /// A scenario in which a whole generated workload arrives at once.
    pub fn from_workload(workload: &Workload) -> Self {
        let accounts = workload
            .ledger
            .accounts()
            .map(|(name, balance)| (name.to_string(), balance));
        Self {
            accounts: accounts.collect(),
            arrivals: BTreeMap::from([(0, workload.transactions.clone())]),
            settings: Settings::default(),
        }
    }

    /// The ledger everything starts from.
    pub fn ledger(&self) -> Ledger {
        Ledger::with_balances(
            self.accounts
                .iter()
                .map(|(name, balance)| (name.as_str(), *balance)),
        )
    }

    /// The last slot in which anything arrives.
    pub fn last_slot(&self) -> u64 {
        self.arrivals.keys().next_back().copied().unwrap_or(0)
    }

    pub fn transaction_count(&self) -> usize {
        self.arrivals.values().map(Vec::len).sum()
    }

    /// Apply the limits this scenario asks for to a configuration.
    pub fn configure(&self, config: &mut NodeConfig) {
        let settings = &self.settings;
        if let Some(units) = settings.block_units {
            config.limits.max_units = units;
        }
        if let Some(count) = settings.block_txs {
            config.limits.max_transactions = count;
        }
        if let Some(threads) = settings.threads {
            config.limits.threads = threads;
        }
        if let Some(ttl) = settings.ttl {
            config.pool.ttl_slots = ttl;
        }
    }
}

/// Parse what follows `tx`: a sender, one or more payments, a fee, and
/// optionally a size and some accounts that are only read.
fn parse_tx(
    words: &[&str],
    id: u64,
    nonces: &mut BTreeMap<String, u64>,
) -> Result<Transaction, String> {
    let mut words = words.iter().copied().peekable();
    let sender = words.next().ok_or("a transaction needs a sender")?;
    if words.next() != Some("pays") {
        return Err("expected `pays` after the sender".into());
    }
    let mut tx = Transaction::new(id, sender, 0);
    loop {
        let payee = words.next().ok_or("expected an account to pay")?;
        let amount = words.next().ok_or("expected an amount")?;
        tx = tx.pay(payee, number(amount, "an amount")?);
        if words.peek() != Some(&"and") {
            break;
        }
        words.next();
    }
    if words.next() != Some("fee") {
        return Err("expected `fee <amount>` after the payments".into());
    }
    let fee = words.next().ok_or("expected a fee")?;
    tx = tx.with_fee(number(fee, "a fee")?);
    if words.peek() == Some(&"units") {
        words.next();
        let units = words.next().ok_or("expected a number of units")?;
        tx = tx.with_units(number(units, "a number of units")?);
    }
    if words.peek() == Some(&"reads") {
        words.next();
        if words.peek().is_none() {
            return Err("expected at least one account after `reads`".into());
        }
        for account in words.by_ref() {
            tx = tx.reading(account);
        }
    }
    if let Some(extra) = words.next() {
        return Err(format!("unexpected `{extra}`"));
    }
    let nonce = nonces.entry(sender.to_string()).or_insert(0);
    tx.nonce = *nonce;
    *nonce += 1;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::random_workload;

    const EXAMPLE: &str = "\
# a small example
account alice 1000
account bob 500   # trailing comments are fine
limit units 300

slot 0
tx alice pays bob 100 fee 10
tx bob pays carol 20 and dave 5 fee 40 units 200 reads oracle prices

slot 2
tx alice pays carol 1 fee 3
";

    #[test]
    fn a_scenario_file_becomes_accounts_and_timed_transactions() {
        let scenario = Scenario::parse(EXAMPLE).unwrap();
        assert_eq!(
            scenario.accounts,
            [("alice".to_string(), 1000), ("bob".to_string(), 500)]
        );
        assert_eq!(scenario.settings.block_units, Some(300));
        assert_eq!((scenario.transaction_count(), scenario.last_slot()), (3, 2));
        assert_eq!(scenario.ledger().balance("bob"), 500);

        let at_start = &scenario.arrivals[&0];
        let first = Transaction::new(1, "alice", 0).pay("bob", 100).with_fee(10);
        assert_eq!(at_start[0], first);
        let second = Transaction::new(2, "bob", 0)
            .pay("carol", 20)
            .pay("dave", 5)
            .with_fee(40)
            .with_units(200)
            .reading("oracle")
            .reading("prices");
        assert_eq!(at_start[1], second);
    }

    #[test]
    fn ids_and_nonces_are_assigned_in_file_order() {
        let scenario = Scenario::parse(EXAMPLE).unwrap();
        let later = &scenario.arrivals[&2][0];
        // The third transaction in the file, and alice's second.
        assert_eq!(
            (later.id, later.sender.as_str(), later.nonce),
            (3, "alice", 1)
        );
    }

    #[test]
    fn transactions_before_any_slot_line_arrive_at_slot_zero() {
        let scenario = Scenario::parse("account a 10\ntx a pays b 1 fee 1\n").unwrap();
        assert_eq!(scenario.arrivals.keys().collect::<Vec<_>>(), [&0]);
        assert_eq!(Scenario::parse("").unwrap(), Scenario::default());
    }

    #[test]
    fn mistakes_are_reported_with_their_line_number() {
        let line_of = |text: &str| match Scenario::parse(text) {
            Err(SchedError::Parse { line, .. }) => line,
            other => panic!("expected a parse error, got {other:?}"),
        };
        assert_eq!(line_of("account alice"), 1);
        assert_eq!(line_of("account alice lots"), 1);
        assert_eq!(line_of("account a 1\naccount a 2"), 2);
        assert_eq!(line_of("account a/b 1"), 1);
        assert_eq!(line_of("\n\nfrobnicate now"), 3);
        assert_eq!(line_of("slot 3\nslot 2"), 2);
        assert_eq!(line_of("limit speed 9"), 1);
        assert_eq!(line_of("limit units many"), 1);
    }

    #[test]
    fn malformed_transaction_lines_say_what_was_expected() {
        let message = |text: &str| Scenario::parse(text).unwrap_err().to_string();
        assert_eq!(
            message("tx alice gives bob 1 fee 1"),
            "line 1: expected `pays` after the sender"
        );
        assert_eq!(message("tx alice pays bob"), "line 1: expected an amount");
        assert_eq!(
            message("tx alice pays bob ten fee 1"),
            "line 1: `ten` is not an amount"
        );
        assert_eq!(
            message("tx alice pays bob 1"),
            "line 1: expected `fee <amount>` after the payments"
        );
        assert_eq!(
            message("tx alice pays bob 1 fee 1 reads"),
            "line 1: expected at least one account after `reads`"
        );
        assert_eq!(
            message("tx alice pays bob 1 fee 1 units 5 extra"),
            "line 1: unexpected `extra`"
        );
        // A line that parses but describes an impossible transaction.
        assert!(message("tx alice pays alice 1 fee 1").contains("it pays its own sender"));
    }

    #[test]
    fn a_scenario_can_set_the_limits_it_needs() {
        let scenario =
            Scenario::parse("limit units 300\nlimit txs 5\nlimit threads 2\nlimit ttl 7").unwrap();
        let mut config = NodeConfig::default();
        scenario.configure(&mut config);
        assert_eq!(
            (config.limits.max_units, config.limits.max_transactions),
            (300, 5)
        );
        assert_eq!((config.limits.threads, config.pool.ttl_slots), (2, 7));

        // Limits a scenario does not mention are left alone.
        let mut untouched = NodeConfig::default();
        Scenario::default().configure(&mut untouched);
        assert_eq!(
            untouched.limits.max_units,
            NodeConfig::default().limits.max_units
        );
    }

    #[test]
    fn a_generated_workload_can_be_run_as_a_scenario() {
        let workload = random_workload(9, 5, 40);
        let scenario = Scenario::from_workload(&workload);
        assert_eq!(scenario.transaction_count(), 40);
        assert_eq!(scenario.ledger(), workload.ledger);
        assert_eq!(scenario.last_slot(), 0);
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        assert!(matches!(
            Scenario::load("/no/such/scenario.txt"),
            Err(SchedError::Io(_))
        ));
    }
}
