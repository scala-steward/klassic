# CLAUDE.md

This file provides guidance for coding agents working in this repository.

## Project Overview

Klassic is a statically typed object-functional programming language implemented
as a native Rust workspace. The default developer path is Cargo-based and builds
the `klassic` executable directly.

Core language areas:
- Hindley-Milner style type inference
- Row-polymorphic records
- First-class functions and closures
- Type classes, constrained polymorphism, and higher-kinded examples
- Lightweight theorem / trust / axiom checking
- Pure Rust runtime helpers for collections, strings, files, directories,
  timing, and threads

## Build Commands

```bash
cargo build
cargo build --release
cargo test
cargo test -p klassic-macro-peg
cargo fmt --check
cargo run -- -e "println(1 + 2)"
cargo run -- path/to/program.kl
```

## Architecture

### Compiler Pipeline

1. Source text
2. `klassic-span`: source files, spans, and diagnostics
3. `klassic-syntax`: lexer, parser, and AST
4. `klassic-rewrite`: placeholder desugaring and syntax normalization
5. `klassic-types`: type inference, record typing, typeclass constraints, and proof checks
6. `klassic-eval`: evaluator, runtime builtins, modules, REPL/session state
7. Root binary: CLI argument handling and diagnostic presentation

### Crates

- `crates/klassic-span`: source location and diagnostic primitives
- `crates/klassic-syntax`: parser and untyped syntax tree
- `crates/klassic-rewrite`: rewrite passes
- `crates/klassic-types`: static checking
- `crates/klassic-eval`: runtime evaluator and builtins
- `crates/klassic-runtime`: shared runtime crate scaffold
- `crates/klassic-macro-peg`: standalone macro PEG parser/evaluator
- `src/`: native CLI binary

## Testing Structure

- Rust unit tests live inside crates.
- Rust integration tests live under `tests/`.
- Klassic sample programs live under `test-programs/` and are exercised by
  the sample-program harness.

Run the full suite before committing core behavior changes:

```bash
cargo fmt --check
cargo test
```

## Language Features

- Space-sensitive list/map/set literals: `[1 2 3]`, `%["a":1 "b":2]`, `%(1 2 3)`
- String interpolation: `"Hello #{name}"`
- Placeholder syntax for anonymous functions: `map([1 2 3])(_ + 1)`
- Cleanup expressions for resource management
- Modules and imports
- Structural records and nominal record declarations
- File, directory, string, numeric, list, map, set, assertion, timing, and thread helpers

## Development Workflow

When adding syntax or semantics:
1. Update `klassic-syntax` for parsing and AST shape.
2. Add or adjust rewrite behavior in `klassic-rewrite` when needed.
3. Extend `klassic-types` for static behavior.
4. Extend `klassic-eval` for runtime behavior.
5. Add focused tests in the relevant crate and integration tests where the user-visible surface changes.
6. Run `cargo fmt --check` and `cargo test`.

## Important Notes

- Keep diagnostics source-span aware.
- Keep tests hermetic; use temp directories for filesystem behavior.
- Do not hardcode sample outputs in the evaluator.
- Keep the default build and runtime path native Rust.
