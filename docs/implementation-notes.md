# Klassic Implementation Notes

## Build And Execution

- Klassic is built as a native Rust workspace.
- The root package produces the `klassic` binary.
- The standard workflow is:

```bash
cargo build
cargo test
cargo run -- -e "1 + 2"
```

## Runtime Model

Klassic currently uses a direct evaluator. The evaluator runs after parsing,
rewriting, and typechecking, and it owns:

- runtime values
- lexical environments
- closures
- records
- modules and imports
- builtin functions
- typeclass dictionaries
- proof trust checks
- REPL/session state

This keeps the implementation straightforward while preserving user-visible
language behavior.

## Type System

The Rust typechecker implements:

- immutable-binding generalization and instantiation
- type annotations
- numeric compatibility checks
- nominal record schemas
- structural record literals and structural record type annotations
- row-polymorphic field constraints
- constrained-polymorphic function declarations such as
  `def display<'a>(x: 'a): String where Show<'a> = show(x)`
- instance declarations with requirements such as
  `instance Show<List<'a>> where Show<'a>`
- higher-kinded typeclass examples over `List`
- theorem / axiom proposition checks and trust-level analysis

## Runtime Surface

The runtime supports:

- `println` and `printlnError`
- numeric helpers such as `sqrt`, `int`, `double`, `floor`, `ceil`, and `abs`
- string helpers such as `substring`, `at`, `matches`, `split`, `join`,
  trimming, replacement, case conversion, `contains`, `indexOf`, `repeat`, and `reverse`
- list, map, and set helpers
- assertions and `ToDo`
- timing helpers
- real Rust threads with synchronized mutable capture snapshots
- file and directory modules using portable Rust filesystem APIs

## REPL

The REPL supports:

- `:exit`
- `:history`
- multiline buffering on incomplete input
- persisted value bindings
- persisted record schemas
- persisted generalized binding schemes and structural record shapes

## Proof Trust

Trust diagnostics are deterministic:

- `--warn-trust` reports trusted proof dependencies.
- `--deny-trust` rejects proof graphs with trusted ancestors.
- Trusted dependencies are reported in source order when multiple candidates exist.

## Future Work

- Move shared runtime components from `klassic-eval` into `klassic-runtime` as
  boundaries become clearer.
- Expand the proof language beyond the current theorem/trust surface.
- Add optimizer or bytecode work only if it improves measurable runtime or
  debugging needs.
