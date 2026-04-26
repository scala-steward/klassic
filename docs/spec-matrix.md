# Klassic Spec Matrix

Status values:
- `not started`
- `partial`
- `complete`
- `verified`

Classification values:
- `required`: part of the supported language/runtime surface
- `optional`: sample or roadmap behavior that is not yet part of the core contract
- `unknown`: needs more repository evidence before being promoted

Rust tests under `tests/` and crate-local unit tests are the executable
authority for this matrix.

| Feature | Classification | Evidence | Rust Status | Verification |
| --- | --- | --- | --- | --- |
| Native CLI entrypoint (`klassic`, `-e`, `-f`, positional `.kl`) | required | `README.md`, `tests/cli_smoke.rs` | verified | `cargo test`, CLI smoke, sample-program harness |
| REPL with `:exit` and `:history` | required | `tests/cli_smoke.rs` | verified | `cargo test` |
| `--deny-trust` / `--warn-trust` | required | `tests/cli_smoke.rs`, evaluator tests | verified | `cargo test`, CLI smoke |
| Line comments | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Nested block comments | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Arithmetic precedence and associativity | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Unary `+` / `-` on integers | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Integer literals | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Long / float / double literals | required | `test-programs/numeric-literals.kl`, `tests/language_regressions.rs` | verified | `cargo test`, sample-program harness |
| Boolean / string / unit literals | required | `tests/language_regressions.rs` | verified | `cargo test` |
| List literals with comma / space / newline separators | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Map literals with comma / space / newline separators | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Set literals with comma / space / newline separators | required | `tests/language_regressions.rs` | verified | `cargo test` |
| String interpolation | required | `test-programs/string-interpolation.kl`, `tests/language_regressions.rs` | verified | `cargo test`, sample-program harness |
| `val` / `mutable` / assignment / compound assignment | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Lambdas and named recursive `def` | required | `README.md`, `tests/language_regressions.rs` | verified | `cargo test` |
| `if`, `while`, `foreach`, ternary | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Cleanup expressions | required | `test-programs/cleanup-expression.kl`, `tests/language_regressions.rs` | verified | `cargo test`, sample-program harness |
| Placeholder desugaring (`_`) | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Records and record field selection | required | `test-programs/record.kl`, `tests/language_regressions.rs` | verified | `cargo test` |
| Row polymorphism and record typing | required | `test-programs/future-features/record_inference.kl`, `tests/language_regressions.rs` | verified | `cargo test` |
| Hindley-Milner inference / schemes / annotations | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Dynamic escape hatch `*` | required | `test-programs/type-cast.kl`, `tests/language_regressions.rs` | verified | `cargo test`, sample-program harness |
| Type classes and instances | required | `tests/language_regressions.rs`, `test-programs/future-features/typeclass-*.kl` | verified | `cargo test`, sample-program harness |
| Higher-kinded type classes / kind annotations | required | `tests/language_regressions.rs`, `test-programs/higher-kinded-typeclass.kl` | verified | `cargo test`, sample-program harness |
| Modules / imports / aliases / selective imports | required | `tests/language_regressions.rs` | verified | `cargo test` |
| User-defined module persistence across evaluations | required | `tests/language_regressions.rs` | verified | `cargo test` |
| File input module | required | `test-programs/file-input.kl`, `tests/language_regressions.rs` | verified | `cargo test`, sample-program harness |
| File output module | required | `test-programs/file-output.kl`, `tests/language_regressions.rs` | verified | `cargo test`, sample-program harness |
| Directory module | required | `tests/language_regressions.rs` | verified | `cargo test` |
| String helper builtins | required | `tests/language_regressions.rs` | verified | `cargo test` |
| List / map / set helper builtins | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Thread / sleep / stopwatch builtins | required | `test-programs/builtin_functions-thread.kl` | verified | `cargo test`, sample-program harness |
| Runtime error helpers (`assert`, `assertResult`, `ToDo`) | required | `tests/language_regressions.rs` | verified | `cargo test` |
| Macro PEG subsystem | required | `klassic-macro-peg`, `tests/language_regressions.rs` | verified | `cargo test -p klassic-macro-peg`, `cargo test` |
| Theorem / trust / axiom surface | required | `tests/cli_smoke.rs`, evaluator tests | verified | `cargo test`, CLI smoke |
| Promoted future-feature programs | optional | `test-programs/future-features/*` | verified | `cargo test --test sample_programs` |

## Current Milestone

The repository contains a Rust-native language implementation with:

- native `klassic` binary via Cargo
- expression/file/REPL execution
- source spans and diagnostics
- parser, rewrite pass, typechecker, evaluator, builtins, modules, and macro PEG
- sample-program and integration-test coverage for the supported language surface

Required behavior identified in this matrix is implemented and backed by Cargo tests.
