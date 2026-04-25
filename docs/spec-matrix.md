# Klassic Spec Matrix

Status values:
- `not started`
- `partial`
- `complete`
- `verified`

Legend:
- `required`: must be preserved in the Rust port
- `optional`: present, but not yet promoted to core parity because repository evidence is weaker
- `out-of-scope`: explicitly excluded from the Rust port
- `unknown`: needs more evidence before committing either way

Evidence notes:
- Historical Scala/JVM source and ScalaTest suites were inspected during the migration and then removed when the Rust port became the only build path.
- Rust tests under `tests/` are now the executable authority for ported behavior.

| Feature | Classification | Repository Evidence | Legacy Status | Rust Status | Verification |
| --- | --- | --- | --- | --- | --- |
| Native CLI entrypoint (`klassic`, `-e`, `-f`, positional `.kl`) | required | `README.md`, historical CLI behavior, `tests/cli_smoke.rs` | complete | verified | `cargo test`, CLI smoke, sample-program harness |
| REPL with `:exit` and `:history` | required | historical REPL behavior, `tests/cli_smoke.rs` | complete | verified | `cargo test` (persisted state + typed reuse through `Evaluator`, CLI smoke for `:history` / `:exit`, multiline buffering) |
| `--deny-trust` / `--warn-trust` flag surface | required | `Main.scala`, `Typer.scala`, `ProofTrustSpec.scala` | partial | verified | `cargo test`, CLI smoke, evaluator/unit tests |
| Line comments | required | `CommentSpec.scala` | complete | complete | `cargo test` |
| Nested block comments | required | `CommentSpec.scala`, `Parser.scala` | complete | complete | `cargo test` |
| Arithmetic precedence and associativity | required | `ExpressionSpec.scala`, `BinaryExpressionSpec.scala` | complete | verified | `cargo test` |
| Unary `+` / `-` on integers | required | `LiteralSpec.scala` | complete | complete | `cargo test` |
| Integer literals | required | `LiteralSpec.scala` | complete | complete | `cargo test` |
| Long / float / double literals | required | `LiteralSpec.scala`, `numeric-literals.kl` | complete | verified | `cargo test`, sample-program harness, `tests/ported_scala_specs.rs` |
| Boolean / string / unit literals | required | `LiteralSpec.scala`, `IfExpressionSpec.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| List literals with comma / space / newline separators | required | `LiteralSpec.scala`, `Parser.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| Map literals with comma / space / newline separators | required | `LiteralSpec.scala`, `MapSpec.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| Set literals with comma / space / newline separators | required | `README.md`, `Parser.scala`, `ModuleImportSpec.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| String interpolation | required | `README.md`, `test-programs/string-interpolation.kl`, `ExpressionSpec.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs`, sample-program harness |
| `val` / `mutable` / assignment / compound assignment | required | `ExpressionSpec.scala`, `TypeCheckerSpec.scala`, `SyntaxRewriter.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| Lambdas and named recursive `def` | required | `README.md`, `ExpressionSpec.scala`, `FunctionSpec.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| `if`, `while`, `foreach`, ternary | required | `ExpressionSpec.scala`, `IfExpressionSpec.scala`, `SyntaxRewriter.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| Cleanup expressions | required | `README.md`, `ExpressionSpec.scala`, `cleanup-expression.kl` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs`, sample-program harness |
| Placeholder desugaring (`_`) | required | `PlaceholderSpec.scala`, `PlaceholderDesugerer.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| Records and record field selection | required | `RecordSpec.scala`, `Ast.scala`, `Typer.scala`, `test-programs/record.kl` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (runtime record access plus static constructor/field checks for concrete schemas, nominal-generic schemas, and structural record literals) |
| Row polymorphism and record typing | required | `Type.scala`, `Typer.scala`, `RecordSpec.scala`, `test-programs/future-features/record_inference.kl`, `examples/typeclass-complete.kl` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (`klassic-types` row-polymorphic field access over unknown record parameters and structural-record dictionary annotations; `klassic-eval` structural record calls + `add_xy` row-polymorphic execution smoke) |
| Hindley-Milner inference / schemes / annotations | required | `TypeCheckerSpec.scala`, `Typer.scala`, `Type.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (`klassic-types` generalizes immutable bindings/defs, instantiates module imports and builtins, preserves polymorphic bindings across REPL turns, supports constrained-polymorphic `def ... where ...` bodies, and keeps annotation checks, undefined-variable checks, mixed integral arithmetic rejection, and nominal-generic record schemas) |
| Dynamic escape hatch `*` | required | `Type.scala`, `Parser.scala`, `Typer.scala`, `type-cast.kl` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (surface parsing plus no-op `:> *` evaluation path) |
| Type classes and instances | required | `TypeClassSpec.scala`, `TypeClassUsageSpec.scala`, `TypeClassSimpleSpec.scala`, `test-programs/future-features/typeclass-polymorphic.kl`, `examples/typeclass-dictionary-passing.md` | partial | verified | `cargo test`, `tests/ported_scala_specs.rs` (runtime dispatch for concrete instances, record instances, multi-method instance bodies, repeated instance override behavior, parser/typechecker/evaluator/CLI coverage for constrained-polymorphic `def ... where Show<'a>` / `where (Show<'a>, Eq<'a>)`, compile-time rejection of missing constrained instances at direct call sites, instance-level `where Show<'a>` constraints, fresh instantiation of one constrained function across `Int` / `String` / record calls, constraint-bound method injection for user functions, and future-feature sample coverage for first-class typeclass methods like `xs.map(show)`, `show(xs)` inside generic constrained code, and pragmatic record/string demos) |
| Higher-kinded type classes / kind annotations | required | `HigherKindedTypeClassSpec.scala`, `Type.scala`, `TYPECLASS_STATUS.md` | partial | verified | `cargo test`, `tests/ported_scala_specs.rs` (parser/runtime slice for `Functor<List>`-style dispatch plus constrained helpers over `'f<'a>` / `'f<'b>` / `'f<'c>` such as `liftTwice`, and runtime dictionary binding for `Monad<'m>` helpers that call result-directed methods like `unit`) |
| Modules / imports / aliases / selective imports | required | `ModuleImportSpec.scala`, `Parser.scala`, `Ast.scala`, `IMPROVEMENTS.md` | partial | verified | `cargo test`, `tests/ported_scala_specs.rs` (`module`, `import`, alias, selective import, exclude import like `Map.{get => _}`, direct selectors, user-module type registry for imports) |
| User-defined module persistence across evaluations | required | `ModuleImportSpec.scala`, `ModuleEnvironment.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` |
| File input module | required | `FileInputSpec.scala`, `test-programs/file-input.kl` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs`, sample-program harness (`FileInput#open`, `readAll`, `readLines`, `all`, `lines`) |
| File output module | required | `FileOutputSpec.scala`, `FileOutputProgramTest.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (`write`, `append`, `exists`, `delete`, `writeLines`, FileInput interop) |
| Directory module | required | `DirSpec.scala`, `PragmaticExampleSpec.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (`current`, `home`, `temp`, `exists`, `mkdir`, `mkdirs`, `isDirectory`, `isFile`, `list`, `listFull`, `delete`, `copy`, `move`) |
| String helper builtins | required | `FunctionSpec.scala`, `StringUtilsSpec.scala`, `BuiltinEnvironments.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (`substring`, `at`, `matches`, `split`, `join`, `trim*`, `replace*`, `case`, `contains`, `indexOf`, `repeat`, `reverse`) |
| List / map / set helper builtins | required | `ListFunctionsSpec.scala`, `MapSpec.scala`, `BuiltinEnvironments.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (`head`, `tail`, `size`, `isEmpty`, `cons`, `map`, `foldLeft`, infix `map`/`reduce`, `Map#*`, `Set#*`) |
| Thread / sleep / stopwatch builtins | required | `BuiltinEnvironments.scala`, `test-programs/builtin_functions-thread.kl` | complete | complete | `cargo test` (`thread`, `sleep`, `stopwatch`, shared mutable capture in evaluator/CLI), sample-program harness |
| Runtime error helpers (`assert`, `ToDo`) | required | `ToDoSpec.scala`, `BuiltinEnvironments.scala` | complete | verified | `cargo test`, `tests/ported_scala_specs.rs` (`assert`, `assertResult`, `ToDo`) |
| Macro PEG subsystem | required | historical macro PEG specs, `tests/ported_scala_specs.rs` | complete | verified | `cargo test -p klassic-macro-peg`, `cargo test` (call-by-name / call-by-value-seq / call-by-value-par examples plus embedded `rule { ... }` surface in main parser/evaluator) |
| Theorem / trust / axiom surface | required | `Parser.scala`, `Typer.scala`, `ProofTrustSpec.scala` | partial | verified | `cargo test`, CLI smoke, syntax/evaluator unit tests (including transitive trust levels, typed theorem/axiom params, Bool/Prop-compatible propositions, proof-body checks, forward proof references, and lightweight proof-term/proposition matching) |
| Java object construction (`new ...`) | out-of-scope | `README.md`, `VmInterpreterSpec.scala`, `ExpressionSpec.scala` | complete | excluded | documented |
| Java method dispatch (`obj->method(...)`) | out-of-scope | `README.md`, `VmInterpreterSpec.scala`, `ExpressionSpec.scala` | complete | excluded | documented |
| JVM helper builtins (`url`, `uri`, `desktop`) | out-of-scope | `BuiltinEnvironments.scala`, `example/desktop.kl` | complete | excluded | documented |
| Pi4J / GPIO integration | out-of-scope | historical build/runtime helpers | complete | excluded by default | documented |
| `test-programs/future-features/**` | optional | `test-programs/future-features/*`, `IMPLEMENTATION_SUMMARY.md` | partial | verified | `cargo test --test sample_programs` covers the promoted Rust-supported future-feature programs |

## Current milestone

This repository now contains the first meaningful Rust-native core:
- native `klassic` binary via Cargo
- `klassic -e "1 + 2"` end to end
- positional `.kl` and `-f` file execution paths
- basic REPL with history and multiline buffering
- integer/double/bool/string/unit literals
- `val` / `mutable`, assignment, and `+=` / `-=` / `*=` / `/=`
- `def`, lambdas, closures, and recursive function calls
- curried call chains for nested lambdas / curried definitions and selected builtins, while non-curried multi-arg functions now reject partial application to match Scala `TypeCheckerSpec`
- placeholder desugaring for `_`, `_ + _`, and `map(xs)(_ + 1)`-style lambdas
- list `map` / `reduce` surface syntax lowered into builtin calls
- trailing brace lambdas such as `map(xs){x => ...}` and zero-arg `=> { ... }` lambdas
- dynamic `:> *` cast surface as a runtime no-op
- `if`, `while`, `foreach`, and `then ... else ...`
- `cleanup` suffix expressions on loops and function bodies
- list literals with comma / space / newline separators
- numeric literal suffix parsing for `BY`, `S`, `L`, and `F`
- string interpolation with embedded expressions
- direct `Module#function(...)` calls for pure Rust modules
- `module ...` headers plus `import`, alias import, selective import, and user-module persistence within the Rust process
- records plus field selection and record constructors
- record field annotations preserved in the AST and used by the Rust typechecker for concrete and nominal-generic constructor / field validation (`#Pair<Int, Int>`-style record references)
- builtin `Point` record plus record-method style self binding for function-valued fields
- first real static `klassic-types` pass wired into evaluation for annotations, immutable binding checks, concrete record schemas, and a subset of numeric compatibility rules
- user-module type exports persisted across evaluations so REPL/import flows keep typechecking while undefined local variables are rejected
- persisted REPL/runtime bindings now seed the Rust typechecker with concrete runtime-derived type hints instead of treating every previous value as `Dynamic`
- first runtime slice for type classes / instances, including higher-kinded `Functor<List>` style examples
- constrained-polymorphic function declarations such as `def display<'a>(x: 'a): String where Show<'a> = show(x)` now parse, typecheck, and execute through the Rust CLI/evaluator, direct calls reject missing instances like `display("nope")`, and repeated calls instantiate freshly across different concrete types
- generic instances with their own requirements such as `instance Show<List<'a>> where Show<'a>` now parse and participate in compile-time constraint solving
- first-class typeclass methods now flow through the Rust evaluator as callable values, so programs like `xs.map(show)` and `items.join(", ")` work inside the future-feature typeclass demos
- dedicated Rust `klassic-macro-peg` crate covering the historical macro PEG specs for call-by-name / call-by-value-seq / call-by-value-par
- theorem / axiom parsing plus trust metadata analysis with enforced `--warn-trust` / `--deny-trust`
- transitive trust-depth reporting (`level 1/2/3...`) fixed under CLI smoke and deterministic `deny-trust` rejection by source order
- integration-test harness that runs the non-JVM top-level `test-programs/*.kl` corpus and checks success plus selected golden stdout
- ported Rust regression coverage for Scala string/list/typeclass/pragmatic/embedded-PEG specs
- builtin `println` / `printlnError`
- first builtin batch for numeric / string / list helpers, plus assertions, timing helpers, and file/dir modules
- first map/set literals and module helper support
- integer arithmetic, comparison, logical operators, and bitwise `&` / `|` / `^`
- line comments and nested block comments

The remaining Java/JVM-specific programs are explicitly out of scope above. Required non-JVM behavior identified in this matrix is implemented on the Rust path and backed by Cargo tests.
