# Rust Port Notes

## Initial decisions

### Native-first build
- The Rust implementation is the only default forward path.
- The historical Scala/JVM implementation has been removed from the repository after parity validation.
- No JVM, Scala, sbt, JNI, JNA, GraalVM, or embedded JVM is permitted in the Rust runtime path.

### Default execution model
- Klassic now uses a direct Rust evaluator instead of re-creating the historical JVM bytecode/VM layers.
- This is an implementation choice, not a language-level behavior change.

### Current milestone scope
- Implemented now:
  - Cargo workspace scaffold
  - native `klassic` binary
  - `-e`, `-f`, positional `.kl`, and REPL shell
  - source spans and diagnostics
  - broad expression/runtime core for functions, control flow, records, modules, and pure builtins
  - line comments and nested block comments
  - standalone Rust `macro_peg` crate covering the historical macro PEG call-strategy specs
  - theorem / axiom parsing plus trust-level warning / deny handling in the Rust evaluator
  - `klassic-types` wired before evaluation, including HM-style schemes, annotations, immutable-binding checks, open-row record constraints, concrete/generic record schemas, constrained typeclass calls, and mixed-integral arithmetic rejection
- The required non-JVM behavior in `docs/spec-matrix.md` is implemented on the Rust path and verified by Cargo tests. Java/JVM interop remains the explicit exclusion boundary.

## Out-of-scope behavior

The following are intentionally excluded from the Rust port unless later repository evidence forces a narrower interpretation:

- Java object construction and method dispatch
- JVM helper builtins whose purpose is Java interop such as `url`, `uri`, and `desktop`
- Pi4J / GPIO behavior in the default build

If hardware support is ever reintroduced, it should be behind an opt-in Cargo feature and must not affect the default build.

## Compatibility notes

- The Rust CLI enforces the current trust surface: trusted theorems / axioms are analyzed before evaluation, `--warn-trust` emits trust warnings, and `--deny-trust` rejects trusted proof graphs.
- Numeric runtime values no longer collapse `Long` and `Float` literals into the generic `Int` / `Double` buckets. The evaluator preserves `2L` as a distinct long value and `2.5F` as a distinct float value while still promoting arithmetic the same way the Rust typer does.
- The Rust integration suite carries direct ports of core legacy evaluator specs (`LiteralSpec`, `FunctionSpec`, `ExpressionSpec`, `BinaryExpressionSpec`, `CommentSpec`, `ListFunctionsSpec`, `MapSpec`), so the broad "expression core" rows in the spec matrix are backed by repository-level regression tests instead of only ad hoc smoke checks.
- The same direct-port strategy covers `ModuleImportSpec`, `FileInputSpec`, `FileOutputSpec`, `DirSpec`, and `StringUtilsSpec`, so module/import behavior and the pure-Rust I/O/string helper surface are no longer justified only by handwritten smoke programs; they are pinned to the original language spec intent.
- That direct-port integration suite now also covers `PlaceholderSpec`, `RecordSpec`, `TypeClassSpec`, `TypeClassSimpleSpec`, `TypeClassUsageSpec`, `HigherKindedTypeClassSpec`, and `ToDoSpec`, while CLI smoke now explicitly exercises `-f`, REPL multiline buffering, and trust-flag behavior. The remaining `partial` rows in the spec matrix are now concentrated in the deeper static typing/proof areas rather than the general runtime surface.
- Rust import parsing supports the legacy `Import` surface: selectors may hide names with `=> _`, so forms like `import hidden.test.{allowed, hidden => _}` keep the alias/selective behavior while excluding the hidden member from the unqualified scope.
- Record declarations preserve field type annotations and nominal generic parameters in the Rust AST so the static checker can reject mismatched record constructors and incompatible field reads for both concrete schemas and `#Pair<Int, Int>`-style record references.
- `klassic-types` now does real immutable-binding generalization/instantiation instead of only one-shot annotation checking: immutable `val` / `def` bindings, builtin signatures, and imported module members are instantiated on use, so polymorphic helpers such as `val id = (x) => x` can be reused at multiple types and survive REPL turns.
- Structural record literals (`record { ... }`) and structural record type annotations (`record { show: ('a) => String }`) are now accepted in the main Rust language pipeline. Row-polymorphic field access is implemented with open-row unification, so functions like `def add_xy(o) = o.x + o.y` typecheck against multiple nominal record shapes and structural dictionary records.
- The Rust parser/typechecker/evaluator now also accept constrained-polymorphic function declarations such as `def display<'a>(x: 'a): String where Show<'a> = show(x)` and `where (Show<'a>, Eq<'a>)`. Typeclass method signatures are preserved in the Rust AST, injected into constrained function bodies during typechecking, executed via the existing runtime instance dispatch, and direct constrained calls now fail early when no matching instance exists.
- Instance declarations can now carry their own requirements, such as `instance Show<List<'a>> where Show<'a>`, and the Rust typechecker recursively satisfies those requirements when resolving direct constrained calls.
- Recursive/constrained `def` bindings are now generalized without leaking their temporary self-binding into the surrounding environment, so one constrained function can be called repeatedly at different concrete types (`Int`, `String`, nominal records) instead of getting accidentally stuck at the first call's type.
- Bare typeclass methods such as `show` and `equals` can now survive identifier evaluation as first-class callable values in the Rust evaluator, which lets future-feature programs like `xs.map(show)` and `items.join(", ")` run instead of failing with `undefined variable`.
- Constraint propagation through generic code is implemented for the repo-backed typeclass surface: with an instance like `instance Show<List<'a>> where Show<'a>`, a constrained helper such as `def showList<'a>(xs: List<'a>): String where Show<'a> = show(xs)` typechecks and runs, and the status sample has been updated accordingly.
- The Rust type representation now keeps generic type application forms such as `'f<'a>` instead of flattening them into opaque names, which is enough for constrained higher-kinded helpers like `def liftTwice<'f, 'a, 'b, 'c>(xs: 'f<'a>, ...) where Functor<'f> = ...` to typecheck and execute for `Functor<List>`.
- Constrained user functions now bind concrete instance dictionaries into their call environment at runtime, so higher-kinded helpers can call methods whose instance cannot be chosen from argument types alone. In practice this means `Monad<'m>` helpers such as `bind(xs, (x) => unit(x + 1))` now work for `List` because `unit` is resolved from the function's constraint environment instead of the direct-call fallback.
- Proof declarations are no longer just metadata in the Rust front-end: theorem/axiom parameters now retain their annotations, proof names are hoisted within a block before typechecking/evaluation, propositions are checked in a lightweight `Bool`/`Prop` bridge, and theorem bodies may be either explicit `Unit` proof scripts like `assert(true)` or proof-term style references such as `base`. Non-`Unit` proof bodies are now checked against the declared proposition by lightweight structural matching and direct proof-signature substitution.
- The Rust typechecker now keeps a lightweight per-process module-type registry in step with the evaluator so REPL state and user-module imports keep working even after undefined-variable checking was tightened.
- Persisted REPL bindings now re-enter the Rust typechecker with runtime-derived type hints (for example `Int`, `String`, homogeneous `List<T>`, named records) instead of being blindly widened to `Dynamic`.
- Persisted REPL state now also replays user-defined record schemas into the Rust typechecker, so a record declared in one turn and constructed in another still exposes typed fields on later turns, including nominal-generic cases like `Pair<'a, 'b>`.
- Persisted REPL type state now also keeps generalized binding schemes and structural record shapes, so multi-turn REPL sessions do not lose polymorphic helper types between evaluations.
- The main Rust parser/evaluator accepts embedded `rule { ... }` blocks as top-level declarations, matching the legacy `EmbeddedMacroPegSpec` surface. Full macro PEG parser/evaluator behavior lives in the dedicated `klassic-macro-peg` crate and covers the call-by-name / call-by-value-seq / call-by-value-par examples.
- Trust diagnostics are now deterministic under `--deny-trust`: when multiple trusted proofs exist, the first trusted proof in source order is reported instead of relying on hash-map iteration order.
- File execution evaluates source and discards the resulting value, preserving the historical CLI behavior for file mode.
- REPL behavior preserves the historical user-visible surface: `:exit`, `:history`, and multiline buffering on incomplete input are present.
- `test-programs/builtin_functions.kl` is now pure Rust-runtime surface. Java/JVM-only samples remain excluded from the default test harness and are documented as out of scope.
- `thread` now spawns a real Rust thread, waits for outstanding spawned work at the end of top-level evaluation, and shares mutable captures through synchronized snapshot cells so CLI/file execution matches the documented "main thread first, worker thread later" behavior without losing simple cross-thread mutation.

## Remaining Future Work

- A later Rust bytecode VM may still be useful for performance, but it is not required for user-visible parity.
- A richer dependent proof language beyond the repository-backed theorem/trust surface remains a future language-design extension, not a blocker for the native Rust port.
