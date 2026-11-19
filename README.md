# ckb-script-fixtures

Executable fixtures for review findings about CKB scripts. Each fixture runs a **naive** and a **hardened** version of a rule in the real CKB script verifier (`ckb-testtool`), so a reader can see the problem and the fix in one `cargo test` run.

This is not a scanner, not an audit and not a security framework. The lock script in `escrow/` is a **toy written for this repository**. It is not the code of any project. A fixture shows how CKB transaction rules behave for that toy; it says nothing about the security of any deployed contract.

## Fixtures (escrow semantics)

| # | Scenario | naive lock | hardened lock |
|---|---|---|---|
| A | Release: two cells with the same lock args in one transaction, only one is "approved" | accepts | rejects (code 13) |
| B1 | Refund with `since = 0` | accepts | rejects |
| B2 | Refund with absolute block number 999, deadline 1000 | accepts | rejects |
| B3 | Refund with absolute block number 1000 | accepts | accepts |
| B4 | Refund with the relative flag set | accepts | rejects (code 30) |
| B5 | Refund with a timestamp metric | accepts | rejects (code 30) |
| C | Refund paying 400 of 1000 (the 600 difference becomes fee for the miner) | accepts | rejects (code 32) |
| D | Anyone creates a cell with the escrow lock, deadline 0 and "approved" state | accepted: no script validates creation | not applicable |
| control | One approved cell, normal release | accepts | accepts |

Results above come from a run of exactly these sources in a private repository (all 5 tests passed). The workflow in this repository re-runs them on every push; see the Actions tab.

## What these fixtures do NOT show

- Anything about a real deployed contract.
- That the hardened lock is complete. It is a minimal fix for the listed cases only.
- Behaviour on mainnet or on a live dev chain (planned, see below).

## Run it

```
rustup target add riscv64imac-unknown-none-elf
cd escrow
RUSTFLAGS="-C link-arg=-s" cargo build --release --target riscv64imac-unknown-none-elf
cd ../tests
ESCROW_BIN=../escrow/target/riscv64imac-unknown-none-elf/release/escrow cargo test --release -- --nocapture --test-threads=1
```

The exact environment variables for the C toolchain are in `.github/workflows/test.yml`.

## Planned

- A fixture for a minimum-capacity remainder case on a local dev chain.
- A fixture for a feed-timestamp upper-bound scenario, only if a standalone reproduction is possible from public source.
- One further public CKB script, tested with its owner's consent. Negative results are published too.

Background: CKB RFC 0017 (transaction valid since), RFC 0022 (transaction structure).

## License

MIT, see `LICENSE`.
