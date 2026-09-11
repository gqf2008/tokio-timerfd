# Contributing

Thanks for improving `tokio-timerd`.

## Development

Run the standard gates from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features realtime
cargo test --features boottime
```

Test `realtime` and `boottime` separately. All timer constructors require an
active Tokio runtime.

## Platform changes

Native timer code is selected by `cfg` in `src/timer.rs`. Changes to a platform
backend must be tested on that platform when possible:

```sh
cargo test
```

Compile-only checks are useful but do not prove timer delivery. Cross-check
FreeBSD and NetBSD with:

```sh
cargo check --target x86_64-unknown-freebsd --all-targets
cargo check --target x86_64-unknown-netbsd --all-targets
```

Do not claim support for a platform that has only been cross-compiled; update
the support table in `README.md` and record the verification method in the PR.

## Benchmarks

Run `cargo bench --bench timer_latency` on real target hardware before making
precision claims. Results are hardware- and load-sensitive.

## Pull requests

Keep changes focused, add regression tests, update public documentation, and
include the exact verification commands and platform results in the PR.
