# Klassic Rust Architecture

## Summary

Klassic is a Rust workspace with small crates for the language pipeline. The
root package builds the native `klassic` binary.

```bash
cargo build
cargo test
cargo run -- -e "1 + 2"
```

## Pipeline

1. Source text
2. `klassic-span`: source files, spans, and diagnostics
3. `klassic-syntax`: lexer, parser, and AST
4. `klassic-rewrite`: placeholder desugaring and syntax normalization
5. `klassic-types`: type inference, records, typeclass constraints, and proof checks
6. `klassic-eval`: evaluator, modules, builtins, and REPL/session state
7. Root binary: CLI argument handling and diagnostic presentation

## Crate Layout

### `klassic-span`
- Source file storage
- Byte spans
- Line/column mapping
- Human-readable diagnostics

### `klassic-syntax`
- Lexer
- Recursive-descent parser
- Untyped AST
- Functions, modules/imports, records, typeclass/instance declarations,
  theorem/trust/axiom declarations, collection literals, and type annotations

### `klassic-rewrite`
- Placeholder desugaring
- Syntax lowering / normalization

### `klassic-types`
- Types, schemes, substitutions, row-polymorphic records, constraints, and typed checks
- Immutable-binding generalization / instantiation
- Builtin and module signatures
- Nominal and structural record typing
- Typeclass and higher-kinded constraints
- Theorem and trust-surface checks

### `klassic-eval`
- Parse -> rewrite -> typecheck -> evaluate wrapper
- Runtime values and evaluator
- Builtins, modules, records, typeclass dictionaries, and thread/file/dir helpers
- REPL/session state

### `klassic-runtime`
- Shared runtime crate scaffold for behavior that should move out of `klassic-eval`
  as the implementation is split further.

### `klassic-macro-peg`
- Standalone macro PEG parser, AST, evaluator, and evaluation-strategy support

### Root Binary `klassic`
- Command-line parsing
- File execution
- REPL
- Exit-code policy and error presentation

## Design Notes

- The direct evaluator is the current execution engine.
- Diagnostics are source-span aware across parse, type, and runtime errors.
- The workspace keeps crate boundaries explicit so future optimizer or runtime
  work can stay isolated.

## Engineering Work

1. Keep Rust tests aligned with newly promoted `.kl` examples.
2. Move shared runtime code from `klassic-eval` into `klassic-runtime` when it
   reduces coupling.
3. Keep CLI and REPL behavior covered by integration tests.
