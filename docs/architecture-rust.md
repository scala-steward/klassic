# Rust Architecture for the Klassic Port

## Summary

Klassic is now a Rust-native workspace with small crates for the compiler pipeline. The current implementation covers the required non-JVM Klassic surface identified from repository tests, examples, and docs: evaluator/runtime core, macro PEG, trust-surface enforcement, and a static typing pass with immutable-binding generalization, instantiation, structural record literals, open-row field constraints, and repo-backed typeclass / higher-kinded behavior.

The root package builds the native `klassic` binary so the default developer flow is:

```bash
cargo build
cargo test
cargo run -- -e "1 + 2"
```

## Crate layout

### `klassic-span`
- Source file storage
- Byte spans
- Line/column mapping
- Human-readable diagnostics

### `klassic-syntax`
- Lexer
- Recursive-descent parser
- Untyped AST
- Current scope: broad Klassic surface including functions, modules/imports, records, typeclass/instance declarations, theorem/trust/axiom declarations, collection literals, and type annotations
- Scope: all non-JVM Klassic syntax currently covered by repo evidence

### `klassic-rewrite`
- Placeholder desugaring
- Syntax lowering / normalization
- Current scope: placeholder desugaring and lightweight normalization for the Rust-supported surface

### `klassic-types`
- Home for kinds, types, schemes, substitutions, row-polymorphic records, typeclass constraints, and typed AST
- Current scope: immutable-binding generalization/instantiation, builtin/module signature schemes, arithmetic compatibility checks, nominal record schemas including generic record references such as `#Pair<Int, Int>`, structural record literals/types, row-polymorphic field constraints, constrained-polymorphic function bodies for `def ... where Show<'a>`-style typeclass usage, repo-backed higher-kinded constraints, and theorem/trust typing

### `klassic-runtime`
- Planned home for runtime values, modules, builtins, filesystem helpers, and concurrency/time helpers
- Scaffolded now; actual behavior will land in later phases

### `klassic-eval`
- Pipeline wrapper for parse -> rewrite -> typecheck -> evaluate
- Current scope: broad non-JVM runtime slice for functions, control flow, records, modules/imports, pure builtins, trust diagnostics, and sample-program execution
- REPL/session state now feeds persisted binding types and persisted record schemas back into `klassic-types`, so later turns can typecheck against earlier user-defined records instead of widening everything to `Dynamic`
- Planned evolution: optional bytecode VM later if needed for performance; the direct evaluator is the parity path.

### `klassic-macro-peg`
- Standalone Rust implementation of the historical `macro_peg` subsystem
- Own parser, AST, evaluator, and evaluation-strategy support
- Kept separate from `klassic-syntax` for now so the PEG-specific tests can move independently of the main language parser

### Root binary `klassic`
- Command-line parsing
- File execution
- REPL
- Exit-code policy and error presentation

## Mapping from historical subsystems

| Historical subsystem | Rust target |
| --- | --- |
| `Main.scala` | root binary `src/main.rs` |
| `Parser.scala` | `klassic-syntax` |
| `Ast.scala` | `klassic-syntax` initially, then split with `klassic-types` for typed IR |
| `PlaceholderDesugerer.scala` | `klassic-rewrite` |
| `SyntaxRewriter.scala` | `klassic-rewrite` |
| `Type.scala`, `TypedAst.scala`, `Typer.scala`, `TypeEnvironment.scala` | `klassic-types` |
| `BuiltinEnvironments.scala`, `Value.scala`, `RuntimeEnvironment.scala` | `klassic-runtime` |
| `Evaluator.scala`, `vm/*` | `klassic-eval` initially, optional later VM crate if it becomes justified |
| `macro_peg/*` | `klassic-macro-peg` |

## Intentional simplifications

- Klassic uses a direct Rust evaluator rather than re-creating the historical JVM VM. A Rust VM is optional, not mandatory, as long as observable behavior matches.
- Diagnostics are first-class from the beginning rather than being reconstructed ad hoc from exceptions.
- Java/JVM interop is not part of the Rust architecture. Out-of-scope surfaces are documented explicitly instead of being left half-working.
- The workspace keeps clear crate boundaries even where the direct evaluator is the parity path, so later VM or optimizer work can remain isolated.

## Remaining Engineering Work

1. Keep Rust tests aligned with any newly promoted `.kl` examples.
2. Consider a Rust VM only if performance or debugging requires it.
3. Treat Java/JVM interop and richer dependent-proof design as separate future work, not part of the default native parity path.
