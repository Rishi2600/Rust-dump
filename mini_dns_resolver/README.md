# mini_dns_resolver

A small DNS resolver written in Rust with no dependencies. Given a name, it
finds the records the way a real recursive resolver does: it starts at the
root servers and follows referrals down the tree until an authoritative
server answers.

It implements:

- the DNS **wire format**: headers, questions, records, and name compression
  in both directions
- record types **A, AAAA, NS, CNAME, MX, TXT, SOA, PTR**, with unknown types
  kept as raw bytes
- **UDP** queries with a timeout, **EDNS0** for larger replies, and a retry
  over **TCP** when a reply is truncated
- **iterative resolution** from the root, including delegations without glue
- **CNAME** following across zones, with loop limits
- **failover** to the next nameserver when one is dead or misbehaving
- a **cache** that honours TTLs and remembers delegations
- basic **forgery defences**: unpredictable query IDs, reply validation, and
  discarding records outside the answering server's zone

## Running it

```sh
cargo run --release -- --trace www.rust-lang.org
```

`--trace` prints each query as a comment line, followed by the records:

```text
; asked 198.41.0.4 for www.rust-lang.org. A: referral to org. (6 nameservers)
; asked 199.19.57.1 for www.rust-lang.org. A: referral to rust-lang.org. (4 nameservers)
; asked 205.251.197.213 for www.rust-lang.org. A: alias for rust-lang.github.io.
; asked 198.41.0.4 for rust-lang.github.io. A: referral to io. (4 nameservers)
; asked 65.22.162.17 for rust-lang.github.io. A: referral to github.io. (5 nameservers)
; asked 198.41.0.4 for dns2.p05.nsone.net. A: referral to net. (13 nameservers)
; asked 192.55.83.30 for dns2.p05.nsone.net. A: referral to nsone.net. (4 nameservers)
; asked 198.51.44.1 for dns2.p05.nsone.net. A: answer, 1 record
; asked 198.51.45.5 for rust-lang.github.io. A: answer, 4 records
www.rust-lang.org.	300	IN	CNAME	rust-lang.github.io.
rust-lang.github.io.	3600	IN	A	185.199.108.153
rust-lang.github.io.	3600	IN	A	185.199.109.153
rust-lang.github.io.	3600	IN	A	185.199.110.153
rust-lang.github.io.	3600	IN	A	185.199.111.153
```

Reading that trace from the top:

1. A root server does not know the name, but refers us to the servers for `org.`
2. An `org.` server refers us to the servers for `rust-lang.org.`
3. That server says the name is an alias for `rust-lang.github.io.`, which is
   in a different zone, so the walk starts again from the root for the new name.
4. The `github.io.` delegation names its servers without giving their
   addresses, so a side lookup resolves `dns2.p05.nsone.net.` first.
5. With that address in hand, the original walk finishes.

The addresses and counts will differ when you run it; this is live data.

### More examples

```sh
cargo run --release -- -t MX example.com          # another record type
cargo run --release -- -x 1.1.1.1                 # reverse lookup (PTR)
cargo run --release -- -s 1.1.1.1 example.com     # ask a recursive resolver instead
cargo run --release -- --trace a.com b.com        # later names reuse the cache
```

### Options

| Option              | Meaning                                              | Default |
| ------------------- | ---------------------------------------------------- | ------- |
| `-t, --type <TYPE>` | record type, by name or as `TYPE<number>`            | `A`     |
| `-x, --reverse`     | treat each name as an IP address and look up its PTR | off     |
| `-s, --server <IP>` | forward to this recursive resolver, do not iterate   | none    |
| `--timeout <MS>`    | how long to wait for each reply                      | 2000    |
| `--trace`           | show every query that is sent                        | off     |

The exit code is 0 when every name resolved, 1 when any failed, and 2 for a
usage error. A name that exists but has no records of the requested type is
not an error; it prints a comment line saying so.

## Source layout

| File              | Role                                                       |
| ----------------- | ---------------------------------------------------------- |
| `src/wire.rs`     | byte reader and writer, big-endian integers                |
| `src/name.rs`     | domain names: parsing, wire form, compression, hierarchy   |
| `src/rtype.rs`    | record type codes and names                                |
| `src/header.rs`   | message header flags, response codes                       |
| `src/question.rs` | the question section                                       |
| `src/record.rs`   | resource records and their typed data                      |
| `src/message.rs`  | whole messages, query building, EDNS, reply validation     |
| `src/transport.rs`| the `Transport` trait, UDP, TCP, and the UDP-to-TCP retry  |
| `src/ids.rs`      | query ID generator                                         |
| `src/roots.rs`    | addresses of the root servers                              |
| `src/cache.rs`    | TTL-aware record cache                                     |
| `src/classify.rs` | what a reply means: answer, alias, referral, or failure    |
| `src/resolver.rs` | forwarding and iterative resolution                        |
| `src/trace.rs`    | transport wrapper that logs every exchange                 |
| `src/main.rs`     | command-line interface                                     |

## Tests

```sh
cargo test
```

The tests never touch the internet. The wire format is tested against
hand-written bytes, the UDP and TCP transports against servers on localhost,
and the resolver against a fake internet held in memory
(`tests/resolver.rs`) with its own root, top-level domains, a delegation
without glue, CNAMEs across zones, dead servers and lying servers.

## Limits

This is a learning project, not a resolver to put in front of real users.

- No DNSSEC: answers are not cryptographically verified.
- Nameservers are contacted over IPv4 only. AAAA records can still be looked up.
- "No such name" answers are not cached, only positive ones.
- Internationalised names must be given in their ASCII (punycode) form.
- One query at a time: no parallel queries and no choosing the fastest server.
