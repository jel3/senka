# Contributing to Senka

Thanks for your interest in contributing.

## Development Setup

```bash
git clone https://github.com/jel3/senka.git
cd senka
cargo build
cargo test
```

To build with the optional TUI:

```bash
cargo build --features tui
```

## Workspace Layout

```
crates/
  core/     — Config models, env resolution, template rendering, redaction
  runner/   — HTTP execution via reqwest
  store/    — SQLite logging (migrations, insert/query/prune)
  secrets/  — OS keychain integration
  cli/      — Clap-based commands and output formatting
  tui/      — (feature-gated) Interactive terminal UI
```

## Making Changes

- Run `cargo clippy` and `cargo fmt` before submitting.
- Run `cargo test` (or `cargo test -p senka-core` for a single crate) to confirm nothing is broken.
- Keep changes focused — one concern per PR.

## Key Constraints

- **Secrets must never appear in files, logs, or stdout** (unless `--no-redact` is explicitly passed). If your change touches secret resolution or output formatting, double-check redaction is applied.
- **No telemetry, no network calls except user-initiated HTTP requests.**
- **TLS verification is on by default.** Any change that weakens this requires explicit discussion.

## Submitting a Pull Request

1. Fork the repo and create a branch from `main`.
2. Make your changes with tests where applicable.
3. Open a PR with a clear description of what the change does and why.

## Reporting Issues

Use [GitHub Issues](https://github.com/jel3/senka/issues). Include:

- Your OS and Rust version (`rustc --version`)
- The command you ran
- What you expected vs. what happened
