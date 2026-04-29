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
7. `klassic-native`: Linux x86_64 native compiler, x64 emitter, and ELF64 writer
8. Root binary: CLI argument handling and diagnostic presentation

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
- Contextual lambda checking for call arguments, including curried reducers
- Typeclass and higher-kinded constraints
- Theorem and trust-surface checks

### `klassic-eval`
- Parse -> rewrite -> typecheck -> evaluate wrapper
- Runtime values and evaluator
- Builtins, modules, records, typeclass dictionaries, and thread/file/dir helpers
- REPL/session state

### `klassic-native`
- Batch native compilation through `klassic build <file.kl> -o <output>`
- Reuses the Rust parser, rewrite pass, typechecker, and proof/trust analysis
- Emits Linux x86_64 machine code directly
- Writes ELF64 executables directly without invoking `cc`, `as`, `ld`, Java, Scala,
  or the JVM
- Current native codegen covers the first vertical slice: integer and boolean
  expressions, string literal printing, `println` / `printlnError`, `assert`,
  curried `assertResult`, `if`, `while`, mutable integer/boolean locals,
  assignment, static integer-list `foreach` unrolling, top-level recursive
  integer functions, static and runtime integer-millisecond `sleep` via Linux `nanosleep`,
  zero-argument literal or lambda-value `stopwatch` via Linux `clock_gettime`, annotated
  boolean-returning / boolean-argument functions, queued `thread` bodies from
  literal or lambda-value jobs for the current native sample surface, simple unannotated
  integer/boolean return inference, stack-passed arguments beyond the first six
  integer/boolean native function parameters, call-site inlined unannotated
  pass-through and string-literal concatenation `def`s over runtime `String` /
  `List<String>` values even when only the return is annotated, annotated
  runtime `String` / `List<String>` parameters for scalar-returning recursive functions, including
  reentrant and self-calls staged before shared parameter buffers are updated,
  fixed-buffer annotated `String` / `List<String>` returns copied into call-site buffers, function
  value aliases, annotated supported record parameters and returns with staged
  record arguments and call-site return copies, including recursive
  runtime-record returns, static record fields, runtime `String` / `List<String>`,
  dynamic `Int` / `Boolean`, and nested runtime record fields, direct or method-style static-list
  `head` lookups including `tail` and `cons` chains, static `Map#get` / `.get`
  lookups with literal or folded static keys preserving those runtime return
  hints and record display paths, and runtime string/int/bool key
  lookups from static maps when the compatible values are strings, string lists,
  ints, booleans, or supported static records, plus block, cleanup,
  and same-runtime-return conditional callees
  preserving those hints,
  immediate calls on conditional function values lowered to branch-local calls,
  pure conditional callable branches in immutable bindings or static aggregates
  lowered to synthesized branch-local-call lambdas for user callables and
  supported builtin function values with matching arity, with conditional
  builtin display metadata preserving selected-branch `<builtin:name>` output
  through printing, interpolation, string concatenation, and `toString`,
  including for returned callables and static aggregates in bound interpolation
  strings,
  top-level and inline lambda calls with the same annotated parameter matching,
  and top-level lambda bindings lowered as static functions or inlined at call
  sites when they capture mutable locals.
  Temporary stack pushes used while evaluating native call arguments are tracked
  alongside local slots, so nested argument expressions can allocate closure
  captures without overwriting saved argument values.
  `println` / `printlnError` stream simple
  string-concatenation and interpolation expressions directly until a heap string
  runtime is added. Interpolated strings can also be folded into static native
  values when all fragments resolve through immutable static bindings, including
  fragments with mutable block prefixes when their final values remain
  recoverable. Interpolation fragments that resolve to native runtime strings,
  or to dynamic native `Int` / `Boolean` values, produce fixed-buffer
  `RuntimeString` values.
  Static string and integer list values can be bound with `val`; static string
  helpers, including `split` and `join`, are folded at compile time for native
  codegen, and static string concatenation can produce immutable static values
  from literals, static bindings, and static helper calls. Runtime string
  concatenation can also format dynamic native `Int` / `Boolean` operands into
  fixed-buffer runtime strings, and `toString` uses the same display path for
  dynamic native `Int` / `Boolean` values plus displayable static-native values
  that survive dynamic/effectful evaluation.
  Int `abs`, `int`, `floor`, and `ceil` are emitted directly. Static
  Double/Float literals and numeric helpers such as `double`, `sqrt`, `abs`,
  `floor`, and `ceil` are folded into printable native constants, with Float
  values kept at f32 precision for evaluator-matching display; helper arguments
  with mutable block prefixes can still be evaluated before recovering the final
  static numeric value. Static integer
  list literals, including simple constant arithmetic and bitwise elements, are
  stored in the native data section and support
  printing plus `size`, `isEmpty`, `contains`, `head`, `tail`, and static `cons` / `map` /
  `foldLeft`. Static non-integer lists live in a compile-time arena and support
  printing, `size`, `isEmpty`, `contains`, `head`, `tail`, `join`, and equality, plus static
  `foreach` unrolling, static `map` over static mappers, and static `foldLeft`
  over static numeric/string accumulator reducers. Static `cons` also covers
  generic static lists. Known Int-list `foreach` bindings are exposed as static
  facts inside the unrolled body, and Int-list `foldLeft` can produce static list
  accumulators. Immutable aliases to directly supported builtin functions, such
  as `val sub = substring`, are resolved back to their builtin call target during
  native codegen, and builtin function values keep evaluator-style
  `<builtin:name>` display when printed directly or inside static aggregates.
  Builtin values stored in static record fields or lists can also be called when
  the recovered arguments stay static. Mutable builtin aliases can be rebound to
  compatible builtin aliases on the straight-line static path.
  Static `if` expressions with recoverable boolean conditions
  can compile only the selected branch, preserving condition side effects before
  producing static native aggregate values, and simple mutable-loop effects are
  tracked for later static folds without folding the generated loop condition.
  Straight-line mutable static strings and generic static lists can be reassigned
  when the new value is also static; straight-line mutable function values can
  likewise be rebound to static lambda / `def` / builtin values. Dynamic `if`
  codegen compiles each branch against an isolated binding, virtual File/Dir,
  and queued-thread state, then merges only facts that are representably
  identical on both paths. Identical static aggregate returns, assignments, or
  virtual file contents can survive a runtime branch. Divergent string and
  runtime line-list branch results are materialized into shared fixed runtime
  buffers so the selected value can flow past the join, and divergent static
  string-list branches can join with each other or with runtime line-list
  branches through the same buffer.
  Function values are merged by structural lambda equality or canonical builtin identity rather than
  raw label identity, so equivalent branch-local function values remain usable.
  When both dynamic branches return equivalent closures that capture branch-local
  mutable slots, their preserved stack depth must match and is carried across
  the join so later closure calls do not reuse the captured storage. Closure
  equality compares only captures that are actually referenced by the lambda
  body, which keeps returned records of equivalent closures mergeable even when
  their surrounding static environments contain unused branch-local names.
  Queued thread bodies use the same structural comparison and stack-depth rule,
  so equivalent branch-local thread captures can also survive a runtime `if`.
  Divergent aggregate / function mutation and divergent queued native threads
  remain compile-time errors. Divergent virtual file state is retained as an
  unknown path; later `FileInput#all` reads and existence/type checks use
  runtime syscalls instead of folding stale build-time facts.
  Logical `&&` and `||` keep normal short-circuit behavior
  and merge static facts conservatively across the skipped/executed RHS.
  Static `map`, `foldLeft`, fold-like three-stage curried calls, direct static
  typeclass methods, and List `bind` / `unit` calls support lambdas that can be
  folded into native constants or data sections. Mapper and reducer callables
  may also be placeholder-derived lambda aliases or top-level functions,
  including static functions that return lambda values, lambda bodies that call
  those returned static closures, and lambda fields that capture dictionary
  records in the typeclass dictionary-passing examples. Top-level lambda
  bindings and top-level `def` declarations are also kept as printable static
  function values. Static lambda values, including mutable aliases and rebinding
  cases, can also be called with runtime integer/boolean arguments by inlining
  the stored body and captured static environment. Non-identifier callees that
  evaluate to a static lambda or builtin function value preserve callee side
  effects before applying the returned callable, including side-effecting builtin
  values like `println`, `sleep`, `assert`, `thread`, and File/Dir helpers that
  update or inspect the native virtual file-system facts, plus string/list
  helpers such as `toUpperCase`, `split`, `join`, and `contains` when their
  arguments are runtime strings or runtime line lists. Supported curried helpers
  returned from effectful callee expressions, such as `assertResult`, `cons`,
  `contains`, `map`, imported `Set#contains`, and three-stage `foldLeft`, preserve
  those callee effects too. Collection and Map/Set helper builtin values such as
  `size`, `head`, `tail`, `isEmpty`, `contains`, `Map#get`, and
  `Set#contains` use the same static helper path when they are called through
  effectful value expressions. Runtime line-list values can also be compared
  against static string-list collection entries, and effectful query values that
  settle back to static values can still use static collection membership.
  Runtime record values copied from static map lookups can be compared
  structurally against static record entries through static list/set `contains`
  and map `containsValue`.
  Static maps can lower `Map#get` / `.get` with runtime string/int/bool keys to
  native comparisons when the compatible values are uniformly string,
  string-list, int, boolean, supported static record, `null`, or `()`, or when
  every compatible entry returns an equivalent static value, including the same callable value; a
  runtime key whose type has no compatible static keys returns static `null`.
  All-`null` compatible values also collapse to static `null`, because hits and
  misses are indistinguishable at the value level. Other runtime misses among
  compatible keys report a native diagnostic because this untagged path cannot
  materialize a dynamic `null`.
  Immediate calls through runtime-key lookups of static callable maps, such as
  `Map#get(fns, key)(...)` and `fns.get(key)(...)`, dispatch to the selected
  lambda or builtin branch and merge the supported native return shapes.
  Runtime string/int/bool-key lookups over all-callable static maps can also be
  stored in immutable values, called later, and formatted through printing,
  interpolation, string concatenation, or `toString` with the same branch
  dispatch; equality involving these function values keeps the evaluator's
  always-false function comparison semantics.
  Lambdas also remember the native stack slots for captured runtime bindings;
  when a block, inline lambda, or call-site inlined function returns such a
  lambda, the captured slots are kept alive so block/function-local mutable
  closure state survives across repeated calls. Static
  records/lists/maps/sets are checked recursively for returned closures, so
  multiple closures stored in a returned record can share the same captured
  mutable slot. Queued native `thread` bodies carry the same runtime capture
  metadata, allowing block-local mutable state to survive until queued thread
  bodies are emitted and run. `thread` itself can queue zero-argument lambda
  values as well as literal lambdas. Functions or static lambdas whose bodies queue
  threads directly or through immutable aliases are compiled on the caller's
  effectful path so queued bodies are attached to the current native execution
  stream rather than to the later function-emission pass or a static fold.
  Non-recursive top-level `def` declarations that close over top-level bindings
  are call-site inlined; recursive `def` declarations can still capture immutable
  static top-level values, builtin aliases, static lambda values, and immutable
  runtime string / line-list bindings by rebinding them inside the emitted
  function frame. Direct calls and value aliases for
  user-defined functions shadow same-named native builtins.
  Immutable aliases to curried helpers such as `assertResult`, `cons`, `map`,
  and `foldLeft` resolve through the same native special-call paths as direct
  helper calls.
  Direct inline lambda calls are compiled at the call site when their arguments
  are runtime integer/boolean values, and impure lambda bodies are kept on the
  runtime path instead of being folded into static constants. Static record
  lambda methods also preserve effectful receivers and arguments when their
  final values remain static. Static lambda values returned from functions can
  be bound and called when captures and arguments are statically recoverable;
  unannotated functions that return runtime-capturing lambdas are inlined at the
  call site so their captured local slots stay alive after the function returns.
  Static `if` values, static binary folds, and
  static call folds use the same purity gate before native folding, and
  dynamic-control assignments invalidate static facts for runtime integer/boolean
  locals. Numeric Float/Double binary expressions and string concatenation can
  still preserve mutable block-prefix effects when their operands ultimately
  yield static values.
  Static equality and `assertResult` over aggregates preserve effectful
  expected/actual expressions while comparing recovered static values, including
  compact Int-list values against generic static lists of Ints and
  side-effecting mixed numeric comparisons whose final values are recoverable.
  User-visible equality treats function and builtin-function values like the
  evaluator does: they compare false even when their native static
  representation is identical. Dynamic branch merging keeps a separate
  structural comparison for closures and builtin function values. Failing
  native `ToDo`, `assert`, `assertResult`, empty-list `head`, negative `sleep`,
  negative string-helper index/count paths, FileOutput open/write syscall
  failures, runtime `Dir#copy` source/target/copy failures, and Dir
  mkdir/delete/move syscall failures write evaluator-style
  source-located diagnostics to stderr before exiting non-zero. Failing
  `assertResult` messages reuse the dynamic print path for conditional builtin
  callable displays. Cleanup expressions preserve
  their body result while still emitting cleanup effects.
  Recoverably false `while` conditions emit their condition effects and skip the
  body, so unreachable native-unsupported constructs do not block compilation.
  Dynamic `while` loops that cannot be simulated to completion also invalidate
  static facts for locals assigned in their condition or body before later
  native folds run.
  Static-list `map` and `foldLeft` can unroll lambdas with mutable prefix
  effects when their final result expression is still statically recoverable;
  method-style `xs.map(f)` and `xs.foldLeft(initial, reducer)` use the same path.
  Static string/Map/Set helper calls may still fold their final helper result
  after emitting impure argument blocks, when those resulting argument values
  are statically recoverable.
  Static `join`, FileInput/FileOutput helpers, and Dir helpers use the same
  side-effect-preserving argument recovery for static paths/content/lists.
  Builtin module aliases, selective imports, and aliased helper values resolve
  to the same native helper paths, so `import Map as M`, `import Map.{size}`, and
  `val readAll = FI#readAll` work in native builds.
  Static `cons` construction and static list/map/set/record literals use that
  argument recovery rule too.
  Static nominal and structural records,
  static map literals, and static set literals with static contents support
  construction, printing, nesting, static map/set helper calls, and equality
  through `assertResult`; records also support field selection, fixed-buffer
  runtime `String` / `List<String>`, dynamic `Int` / `Boolean`, and nested runtime
  record fields, compatible record equality and runtime string display for those fields,
  dynamic `if` merging for compatible runtime record branch results, mutable
  runtime record assignments from runtime or supported static initializers,
  annotated record function parameters/returns over the same field storage, and
  static lambda method fields.
  Static file input/output helpers for static paths are supported
  with Linux syscalls and compile-time virtual file tracking; `FileOutput#write`
  / `FileOutput#append` can also write fixed-buffer runtime string content.
  Static-path `FileInput#open` callback bodies and callable callback values bind
  the stream path before normal native compilation, allowing them to return
  supported runtime values as well as folded static values.
  Paths whose contents become unknown through runtime writes or dynamic branches
  fall back to runtime `FileInput#all`, `FileInput#lines` / `readLines`,
  `FileOutput#exists`, `Dir#exists`, `Dir#isFile`, `Dir#isDirectory`,
  `Dir#list`, and `Dir#listFull` syscalls.
  Runtime string values can also
  be copied into NUL-terminated syscall path buffers for `FileInput#all` and
  direct file-input printing. `FileInput#open` callbacks with runtime paths bind
  the stream parameter as a runtime string, so callback bodies and callable
  callback values can return it or pass it through supported runtime string and
  file helpers such as `readAll`, `readLines`, `length`, and `cleanup`.
  Mutable runtime string and line-list bindings copy assignments into fixed
  buffers, allowing loop-carried string accumulators, line-list cursors, and
  closures that observe later assignments.
  Direct printing or immutable printable bindings of `FileInput#lines` / `readLines`
  are also supported, with `size`, `isEmpty`, `head`, `tail`,
  `cons`, inline or aliased-lambda and builtin-function-value `map` producing
  string line lists,
  String/Int/Bool/Null/Unit/List<String>-accumulator direct or method-style `foldLeft` with inline or aliased reducers, `join`,
  `split` / `join` with static or runtime string delimiters on runtime strings,
  runtime `foreach`, and
  equality / `assertResult` support
  against static string lists or other runtime line lists, plus
  `FileOutput#writeLines` write-back for runtime line lists,
  `FileOutput#write` / `append` / `writeLines` / `exists`, and
  `FileOutput#delete`, plus `Dir#mkdir` / `mkdirs` / `delete` / `copy` /
  `move` and `Dir#exists` / `isFile` / `isDirectory` / `list` / `listFull`.
  Runtime directory listings are represented as runtime line lists and sorted to
  match static/evaluator directory listing order.
  Direct
  `println(FileInput#all(path))` / `println(FileInput#readAll(path))` streams
  runtime file content without requiring the file to exist at native build time.
  Immutable runtime `FileInput#all(path)` / `readAll(path)` bindings can be
  printed or concatenated through a fixed native string buffer, compared with
  `==` / `!=` or `assertResult`, and queried with `isEmptyString` / `length`;
  method-style `toString`, `substring` / `at` with static or runtime integer
  indexes, ASCII-whitespace `trim` / `trimLeft` / `trimRight`, `repeat` with
  static or runtime integer counts,
  ASCII `toLowerCase` / `toUpperCase`,
  simple `matches` with static or runtime patterns, first-occurrence `replace`
  with static or runtime literal operands, all-occurrence `replaceAll` with
  static or runtime pattern and replacement strings, UTF-8 `reverse`,
  `startsWith`, `endsWith`, method-style `contains`, `indexOf`, and
  `lastIndexOf` are also supported. Oversized results fail with
  source-located runtime diagnostics. `FileInput#open`
  callback folding preserves mutable callback effects when final values remain
  statically recoverable. `Dir#current()` emits runtime `getcwd` and returns a
  runtime string so generated executables observe their execution cwd.
  `Dir#home()` reads runtime `HOME`, while `Dir#temp()` reads runtime `TMPDIR`
  with `/tmp` as its Linux fallback.
  `CommandLine#args()` reads the generated executable's argv at runtime,
  excludes argv[0], and exposes the result as a runtime line list for direct,
  unqualified, aliased-helper, and function-local native calls.
  `Process#exit(code)` evaluates its code argument and emits the Linux process
  exit syscall, giving generated native CLI tools explicit status codes. Static
  strings also route `substring` / `at` through the runtime slice emitter when
  the index expressions are mutable or otherwise dynamic integers, and static
  string `split` plus static string-list `join` accept runtime string
  delimiters through the same runtime string buffer path. Static
  first-occurrence `replace` can also use runtime string pattern and replacement
  operands, and static `repeat` accepts runtime integer counts.
  `StandardInput#all()` / `stdin()` read stdin into a fixed-buffer runtime
  string, and `StandardInput#lines()` / `stdinLines()` expose stdin through the
  runtime line-list representation shared with file and argv helpers. Static
  `Environment#vars()` / `env()` expose the generated executable's environment
  as `KEY=VALUE` runtime line-list entries for direct, aliased-helper, and
  generated-function native calls. `Environment#get(name)` / `getEnv(name)` and
  `Environment#exists(name)` / `hasEnv(name)` scan that same saved envp table for
  direct variable lookup and existence checks with static or runtime string keys.
  Static
  `Dir` helpers cover existence/type checks, mkdir/mkdirs, list/listFull,
  delete, copy, and move on static paths. Static `null` is available for
  immutable bindings, printing, equality, and `Map#get` misses. `()` is
  available for immutable bindings, printing, static string concatenation,
  equality, and `assertResult`. Native
  `assertResult` covers integers, booleans, static strings, static integer
  lists, static records, static maps, static sets, static nulls, and unit.
  Ordinary `==` / `!=` covers runtime integers/booleans and static aggregate
  values. `ToDo()` emits the evaluator-compatible native runtime failure text.

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
- The native compiler is an additional batch build engine. Unsupported language
  constructs fail at compile time with span-aware diagnostics rather than falling
  back to the evaluator.
- The ELF writer computes the data segment offset from the actual text length and
  page-aligns it, so larger generated programs do not overwrite or truncate text.
- Diagnostics are source-span aware across parse, type, and runtime errors.
- The workspace keeps crate boundaries explicit so future optimizer or runtime
  work can stay isolated.

## Engineering Work

1. Keep Rust tests aligned with newly promoted `.kl` examples.
2. Move shared runtime code from `klassic-eval` into `klassic-runtime` when it
   reduces coupling.
3. Keep CLI and REPL behavior covered by integration tests.
