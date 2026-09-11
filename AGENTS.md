# Repository Guidelines

## Project Structure & Module Organization

`tokio-timerd` is a cross-platform Rust timer library for Tokio. `src/lib.rs` defines `TimerFd`, `sleep`, and the public backend documentation. `src/delay.rs` implements the `Delay` future, `src/interval.rs` implements the `Interval` stream, and `src/timer.rs` selects the platform backend. Linux/Android uses `timerfd`; Windows uses a high-resolution waitable timer with a standard-timer fallback; BSD/macOS uses `kqueue`; other targets fall back to Tokio timers. `src/delay_queue.rs` implements the multi-entry `DelayQueue` over one native timer. Unit tests are embedded beside each module.

## Build, Test, and Development Commands

Run commands in the repository root:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features realtime
cargo test --features boottime
cargo bench --bench timer_latency
```

The interval tests use real OS timers and intentionally tolerate scheduler jitter. Benchmarks are hardware-sensitive; use `BENCH_ITERATIONS` to increase sample size. Test the Linux clock features separately; enabling both is invalid. `Cargo.lock` is intentionally ignored for this library. Cross-check supported targets with `cargo check --target x86_64-unknown-freebsd --all-targets` or `--target x86_64-unknown-netbsd`.

## Coding Style & Naming Conventions

Follow Rust 2018 and rustfmt defaults (four-space indentation and standard import grouping). Use `snake_case` for functions and modules, `UpperCamelCase` for types, and descriptive feature names. Keep platform-specific code behind `cfg` modules in `src/timer.rs`; do not leak Unix-only types into cross-platform APIs. Document public items with `///` or crate-level `//!` comments. Validate all FFI handles, descriptors, and syscall return values. Use `// SAFETY:` comments for `unsafe` blocks.

## Testing Guidelines

Tests use `#[cfg(test)]` modules, `#[tokio::test]` for async behavior, and plain `#[test]` for deterministic helpers. Name tests for the behavior and expected result, such as `delay_reports_elapsed_state` or `missed_ticks_advance_to_next_boundary`. Add or update a focused test beside changed code; no coverage threshold is enforced. Cover native backends where CI runs them and keep timing assertions tolerant.

## Commit & Pull Request Guidelines

History uses short imperative subjects, with Conventional Commit prefixes in newer work (`refactor: ...`, `chore: ...`). Keep each commit scoped and explain why the change is needed. In each pull request, summarize behavior, link the related issue, list exact verification commands and results, and identify affected platforms or clock features. Update public documentation when APIs or platform support changes.
