# Repository Guidelines

## Project Structure & Module Organization
- Cargo workspace written in Rust.
- Native CLI binary: `src/main.rs` and `src/cli.rs`.
- Compiler/runtime crates: `crates/klassic-*`.
- Integration tests: `tests/`.
- Klassic sample programs and golden fixtures: `test-programs/`, `examples/`, and `example/` if present.
- Architecture and specification docs: `docs/`.

## Build, Test, And Development Commands
- Build: `cargo build`.
- Release build: `cargo build --release`.
- Run expression: `cargo run -- -e "1 + 2"`.
- Run file: `cargo run -- path/to/file.kl` or `cargo run -- -f path/to/file.kl`.
- REPL: `cargo run`.
- Test: `cargo test`.
- Macro PEG tests only: `cargo test -p klassic-macro-peg`.
- Formatting check: `cargo fmt --check`.

## Coding Style & Naming Conventions
- Language: Rust 2024 edition.
- Use idiomatic Rust modules, enums, structs, and pattern matching.
- Keep diagnostics and source spans first-class.
- Avoid `unsafe` unless absolutely necessary and documented.
- Prefer small cohesive crates/modules over broad catch-all files.
- Default to ASCII in source and docs unless the file already justifies Unicode.

## Testing Guidelines
- Language behavior should be represented by Rust tests.
- Add focused tests for parser, rewrite, typing, runtime, CLI, REPL, and golden `.kl` programs.
- Use hermetic temp directories for file and directory module tests.
- Run `cargo test` before committing core language changes.

## Commit & Pull Request Guidelines
- Commits: imperative mood, concise subject under 72 characters when practical.
- PRs: include what changed, why, user-visible impact, and validation.
- CI must be green on the Rust-native path.

## Security & Publishing Notes
- Do not commit secrets or machine-local configuration.
- Keep the default build and runtime path native Rust.

## Agent-Specific Instructions
- Prefer `rg` for search.
- Use Cargo commands for validation.
- Keep docs synchronized when language behavior or implementation scope changes.
