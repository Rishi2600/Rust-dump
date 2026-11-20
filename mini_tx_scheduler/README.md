# mini_tx_scheduler

A transaction pool and block builder written in Rust with no dependencies. It
takes in transactions that each offer a fee, decides which run first, and
arranges them so that ones touching different accounts run at the same time.

The ordering rule is deliberately simple:

> **The highest fee per compute unit goes first. Ties go to whoever arrived
> first.**

Everything else in the project is what it takes to apply that rule safely:

- a **pool** that holds waiting transactions per sender, in nonce order, with
  admission checks, replace-by-fee, a size limit with eviction, and expiry
- a **priority queue** (a binary heap written from scratch) over the one
  transaction per sender that is ready to run
- **conflict detection** from the accounts each transaction declares it reads
  and writes
- a **batch planner** that puts clashing transactions in separate batches, in
  priority order, and lets the rest run in parallel
- a **block builder** with a compute budget, so space is scarce and fees
  matter
- an **executor** that runs each batch on several threads, and a serial one to
  check it against
- two alternative rules, total fee and first come first served, to measure
  the default against

## Running it

```sh
cargo build --release
target/release/mini_tx_scheduler scenarios/basics.txt
```

```text
6 transactions, blocks of 10000 units and 4 threads, ranked by fee-per-unit

slot 0: 6 transactions in 4 batches, 900 units, fees 680
  batch 0
    #2   bob -> shop 200                    fee   80  units  100   0.80/unit  ok
    #3   carol -> frank 50                  fee   40  units  100   0.40/unit  ok
  batch 1
    #5   dave -> shop 900                   fee   20  units  100   0.20/unit  FAILED: insufficient funds
  batch 2
    #1   alice -> shop 100                  fee   10  units  100   0.10/unit  ok
  batch 3
    #4   alice -> grace 5                   fee  500  units  100   5.00/unit  ok
    #6   erin -> heidi 10 (reads shop)      fee   30  units  400   0.07/unit  ok
```

Reading that block from the top shows each rule at work:

1. **Bob goes first.** He pays 0.80 per unit, the best rate among the
   transactions that are ready.
2. **Carol runs alongside him.** She touches none of the accounts he does, so
   the two share a batch and run in parallel.
3. **Dave waits for the next batch.** He also pays the shop, which Bob's
   transaction is changing. Dave has 50 and tries to send 900, so his
   transaction fails. He is still charged his fee.
4. **Alice's first comes after Dave's.** It pays less (0.10) and clashes with
   it over the shop.
5. **Alice's second pays the most of all (5.00) and still runs last of hers.**
   A sender's transactions run in the order they were sent.
6. **Erin only reads the shop.** She waits until the last write to it is done,
   then shares a batch with Alice's second, which touches other accounts.

### Why fee per unit

`scenarios/congestion.txt` has more transactions than fit. Blocks hold 300
units and nothing may wait more than two slots. `--compare` runs it under
each rule:

```text
$ target/release/mini_tx_scheduler scenarios/congestion.txt --compare
9 transactions, blocks of 300 units and 4 threads, ranked by each policy in turn

policy           slot 0   slot 1   slot 2     fees  included  expired
fee-per-unit        320      150        0      470         4        5
total-fee           150      140        0      290         2        7
fifo                 30      150        0      180         4        5
```

Ranking by total fee fills each block with one large transaction that pays a
lot in total but little for the space it takes. First come first served
ignores fees altogether. Ranking by rate fills the first block with three
small transactions that each pay well for their size.

### A generated workload

`--random` makes up transactions instead of reading a file. The same seed
always gives the same ones.

```text
$ target/release/mini_tx_scheduler --random 7 --count 400 --accounts 12 --block-units 6000 --ttl 3 --compare
400 transactions, blocks of 6000 units and 4 threads, ranked by each policy in turn

policy           slot 0   slot 1   slot 2   slot 3     fees  included  expired
fee-per-unit       5322     2738     2915        0    10975        68      332
total-fee          4397     2549     3290        0    10236        63      337
fifo               4162     1971     2858        0     8991        60      340
```

### Options

| Option | Meaning | Default |
| --- | --- | --- |
| `--policy <NAME>` | `fee-per-unit`, `total-fee` or `fifo` | `fee-per-unit` |
| `--compare` | run under every policy and show the fees side by side | off |
| `--threads <N>` | transactions that may run at once | 4 |
| `--block-units <N>` | compute units per block | 10000 |
| `--block-txs <N>` | transactions per block | 1000 |
| `--ttl <SLOTS>` | slots a transaction may wait before it is dropped | 32 |
| `--random <SEED>` | use a generated workload instead of a file | |
| `--accounts <N>`, `--count <N>` | size of a generated workload | 10, 60 |
| `--summary` | print only the totals | off |

## Scenario files

A scenario is a plain text file. `#` starts a comment.

```text
account alice 1000          # a starting balance
account bob 500
limit units 300             # optional: units, txs, threads or ttl

slot 0                      # what follows arrives before block 0 is built
tx alice pays bob 100 fee 10
tx bob pays carol 20 and dave 5 fee 40 units 200 reads oracle

slot 2
tx alice pays carol 1 fee 3
```

A `tx` line is a sender, one or more payments joined by `and`, a fee, and
optionally a size in units (100 if left out) and accounts it only reads.
Transactions are numbered from 1 in file order, and each sender's nonces
count up from 0, so neither needs writing out.

Three scenarios are included: `basics.txt` (above), `congestion.txt`, and
`nonces.txt`, which shows one sender's transactions spread over several
blocks.

## The rules, in one place

| Rule | What it means |
| --- | --- |
| Priority | fee divided by units, compared exactly; earlier arrival breaks ties |
| Sender order | a sender's transactions run in nonce order, whatever they pay |
| Conflicts | two transactions clash if one writes an account the other reads or writes |
| Batches | clashing transactions go in separate batches, the higher priority first |
| Block budget | a block holds a fixed number of compute units; what does not fit waits |
| Fees | charged whether a transaction succeeds or fails; paid to the validator once per block |
| Replacement | the same sender and nonce can be re-sent with a fee at least 10% higher |
| Full pool | a newcomer may push out the lowest-ranked transaction, if it outranks it |
| Expiry | a transaction that waits too many slots is dropped |

## Source layout

| File | Role |
| --- | --- |
| `src/ledger.rs` | balances and nonces |
| `src/tx.rs` | the transaction, its validation, and its read and write sets |
| `src/priority.rs` | the fee rate comparison and the three policies |
| `src/heap.rs` | a binary max-heap |
| `src/mempool.rs` | the pool: admission, replacement, eviction, expiry, readiness |
| `src/conflict.rs` | conflict detection and the batch planner |
| `src/scheduler.rs` | the block builder |
| `src/executor.rs` | running transactions, serially and in parallel |
| `src/node.rs` | the slot loop: pool, block, execution, fees |
| `src/workload.rs` | repeatable random workloads |
| `src/scenario.rs`, `src/sim.rs` | scenario files, running them, comparing policies |
| `src/report.rs` | text output |
| `src/main.rs` | command-line interface |
| `scenarios/` | sample scenarios |

## Tests

```sh
cargo test
```

Each module has unit tests on examples chosen by hand. Two test files go
further, on thousands of generated transactions:

- `tests/invariants.rs` checks properties every schedule must have: no batch
  holds two clashing transactions, clashing ones keep their priority order,
  each sender's nonces are in order without gaps, every limit is respected,
  and a better transaction is only chosen later when its own sender's queue
  held it back.
- `tests/serializability.rs` checks the claim the design rests on: running a
  block's batches in parallel leaves exactly the same ledger as running its
  transactions one at a time in priority order. It also checks that the
  number of threads never changes the result, and that no money is created
  or lost.

## Limits

This is a learning project. It models how a scheduler decides, not a real
chain.

- Transactions only transfer balances. There are no programs or signatures.
- Picking by fee rate is greedy. It is usually good and not always the best
  possible fill of a block.
- A transaction cannot pull an earlier one from the same sender forward by
  paying more ("child pays for parent").
- Only successful or expired transactions leave the pool. Nothing tells the
  sender why theirs was dropped.
- One node, no network, no competing block builders.
