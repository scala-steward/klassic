# Klassic

Klassic is a statically typed object-functional programming language written in
Rust. The implementation builds a native `klassic` executable with Cargo.

## Features

- Hindley-Milner style type inference with annotations and generalized schemes
- Row-polymorphic records and record field selection
- First-class functions, closures, named recursive functions, and mutable locals
- Type classes, constrained polymorphism, and repository-backed higher-kinded examples
- Lightweight theorem / trust / axiom surface with `--warn-trust` and `--deny-trust`
- String interpolation, comments, cleanup clauses, loops, ternary expressions, and casts
- List, map, and set literals with comma, space, or newline separators
- Pure Rust file, directory, string, list, map, set, time, and thread helpers
- Native CLI and REPL
- Standalone Rust macro PEG subsystem

## Build And Test

Install a recent Rust toolchain, then use the normal Cargo workflow:

```bash
cargo build
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo run -- -e "1 + 2"
```

Build an optimized native executable:

```bash
cargo build --release
./target/release/klassic -e "1 + 2"
```

## CLI

Evaluate an expression:

```bash
klassic -e '1 + 2'
```

Run a file:

```bash
klassic path/to/program.kl
klassic -f path/to/program.kl
```

Build a Linux x86_64 native executable:

```bash
klassic build path/to/program.kl -o program
./program
```

Start the REPL:

```bash
klassic
```

REPL commands include `:history` and `:exit`.

Trust diagnostics:

```bash
klassic --warn-trust proofs.kl
klassic --deny-trust proofs.kl
```

`--warn-trust` reports trusted proof dependencies. `--deny-trust` rejects any
proof graph that depends on a trusted theorem or axiom.

The native compiler currently supports the first direct-ELF vertical slice:
integer and boolean expressions, string literal printing, `println`,
`printlnError`, `assert`, curried `assertResult`, `if`, `while`, mutable
integer/boolean locals, assignment, static integer-list `foreach` unrolling,
static and runtime integer-millisecond `sleep` via Linux `nanosleep`,
zero-argument lambda `stopwatch` via Linux `clock_gettime`,
queued `thread` bodies for the current native sample surface,
top-level recursive integer functions, and annotated boolean-returning /
boolean-argument functions. Obvious unannotated integer/boolean function and
top-level lambda return values are inferred for native codegen. Native function
calls support stack-passed arguments beyond the first six integer/boolean
parameters, while native codegen tracks its own temporary stack slots so nested
argument evaluation can allocate local captures safely. Top-level lambda
bindings are lowered as static functions or inlined at call sites when they
capture mutable native locals, and direct inline lambda calls can receive
runtime integer/boolean arguments without folding away impure lambda bodies.
Static lambda values returned from functions can be bound and called again when
their captured values and call arguments are statically recoverable.
Static record lambda methods follow the same rule, so native method calls keep
mutable side effects on the runtime path, including effectful receiver and
argument expressions when their final values remain statically recoverable.
Static `if` folding is likewise limited to pure conditions and selected
branches, keeping mutable branch effects in generated code.
Assignments to runtime integer/boolean locals inside dynamic control flow also
clear stale static facts for those locals.
Dynamic `while` loops that cannot be fully simulated also invalidate static
facts for locals assigned in the loop condition or body before later expressions
are folded.
Static binary folds use the same purity check before replacing numeric,
equality, or string-concatenation expressions.
Numeric Float/Double binary expressions can also preserve mutable block-prefix
effects when both sides recover static numeric values.
String concatenation can still preserve mutable block-prefix effects when a
side ultimately yields a static string value, and native `+` concatenation can
now build fixed-buffer runtime strings when a static/runtime string is combined
with dynamic native `Int` or `Boolean` values.
Logical `&&` and `||` preserve normal short-circuiting in generated native code,
including static fact tracking, so skipped RHS effects are not used by later
native folds.
Static call folding and `assertResult` folding also require pure arguments, so
argument blocks with mutable effects are evaluated by generated code. Immutable
aliases to curried helpers such as `assertResult`, `cons`, `map`, and `foldLeft`
lower to the same native paths as direct calls.
Static equality and `assertResult` over aggregate values also preserve
side-effecting expected/actual expressions while comparing recovered values.
Static-list `map` and `foldLeft` can also unroll lambdas with mutable prefix
effects when the final lambda result remains statically recoverable; method
style `xs.map(f)` uses the same native path.
Static string/Map/Set helper calls that can still determine static argument
values after evaluating impure argument blocks preserve those generated side
effects before folding the helper result.
Static `join`, FileInput/FileOutput path/content/list arguments, and Dir path
arguments follow the same rule when the resulting arguments remain statically
recoverable. Static `cons` head and tail arguments use that rule too, so list
construction does not fold away mutable block effects.
Static list, map, set, record literal, and record constructor arguments use the
same recovery path.
`println` and
`printlnError` can also stream simple string-concatenation expressions and
string interpolation fragments without a heap string runtime yet. Interpolated
strings whose fragments depend on immutable static values can also be folded for
native `val` bindings and `assertResult`; fragments containing runtime native
strings, or dynamic native `Int` / `Boolean` fragments, produce fixed-buffer
`RuntimeString` values. Mutable block prefixes inside fragments are preserved
when their final values remain statically recoverable.
Static string and integer list values
can be bound with `val`; static string helper calls such as `substring`, `split`,
`join`, `trim`,
`replace`, `startsWith`, `indexOf`, `length`, `repeat`, and method-style
`"text".contains("x")` are folded during native compilation. Static string
helper functions can also be bound through immutable aliases such as
`val sub = substring` and called through that alias in native builds.
Runtime `FileInput#all` / `FileInput#readAll` bindings support the same fixed
native string buffer path for printing, concatenation, equality, `assertResult`,
method-style `toString`, `substring` / `at` with static or runtime integer
indexes, ASCII-whitespace trimming, `length`, `repeat` with static or runtime
integer counts, and string search predicates, plus ASCII `toLowerCase` /
`toUpperCase`, simple `matches` with static or runtime patterns,
first-occurrence `replace` with static or runtime literal operands,
all-occurrence `replaceAll`, and UTF-8 `reverse`.
Static string
concatenation can be used in immutable bindings and static record fields when at
least one operand is a static string, including static helper calls such as
`size`, `head`, and method-style `parts.size()`. Runtime string concatenation
also accepts dynamic native `Int` / `Boolean` operands by formatting them into
the fixed string buffer. Integer `abs`, `int`, `floor`, and `ceil` calls are
emitted for Int arguments, and static Double/Float literals
plus static numeric helpers such as `double`, `sqrt`, `abs`, `floor`, and `ceil`
are folded into printable native constants, with Float values preserving f32
rounding and display; helper arguments with mutable block prefixes are evaluated
before recovering the final static numeric value. Static integer list literals,
including simple constant arithmetic elements, are emitted into native data
sections and can be printed or passed to `size`, `isEmpty`,
`head`, `tail`, and static `cons` / `map` / `foldLeft`; static non-integer lists
are represented in compile-time arenas and support printing, `foreach`
unrolling, `size`, `isEmpty`, `head`, `tail`, `join`, static `map` for static
mappers, static `foldLeft` for static accumulator reducers such as string
concatenation, generic static `cons`, ordinary `==` / `!=`, and `assertResult`.
Known Int-list `foreach` bindings are also available to static folds inside the
loop body, so native code can build static lists and records from the iteration
value.
Int-list `foldLeft` can also build static list accumulators, such as reverse via
`e #cons acc`.
Static `if` expressions whose conditions fold to booleans can produce static
strings, lists, records, maps, sets, null, or unit values.
Simple mutable loops are tracked for later static folds when the generated loop
code remains dynamic. Static `map`, `foldLeft`, fold-like three-stage curried
calls, direct static typeclass methods, and List `bind` / `unit` calls support
integer and static numeric/string lambdas that can be folded into native
constants or data sections, including lambdas that call returned static closure
values. Static structural and nominal records, static
map literals, and static set literals can also be bound, printed, nested, queried
with static map/set helpers, and compared with `assertResult` when their
contents are static native values. Builtin module aliases, selective imports,
and aliased helper values, such as `import Map as M`, `import Map.{size}`, and
`val readAll = FI#readAll`, resolve to the same native helper implementations.
Static record fields may contain lambda
methods that are called with the receiver for native static evaluation. Static
file input/output helpers for static paths are supported through Linux file
syscalls plus compile-time virtual file tracking; `FileOutput#write` /
`FileOutput#append` can also write fixed-buffer runtime string content. Paths
whose contents become unknown through runtime writes or dynamic branches
fall back to runtime `FileInput#all`, `FileOutput#exists`, `Dir#exists`,
`Dir#isFile`, `Dir#isDirectory`, `Dir#list`, and `Dir#listFull` syscalls.
Runtime string values can also be
used as paths for `FileInput#all`, direct file-input printing, simple
`FileInput#open(path, stream => FileInput#readAll(stream))` callbacks, and
direct printing or immutable printable bindings of `FileInput#lines` /
`readLines` including matching simple `open(...readLines...)` callbacks; those
runtime line lists support `size`, `isEmpty`, `head`, `tail`, `cons`, `map`,
with inline or aliased lambdas, String-accumulator `foldLeft` with inline or
aliased reducers, `split` / `join` with static or runtime string delimiters on
runtime strings, runtime `foreach`, and equality /
`assertResult` checks against static string
lists or other runtime line lists, and `FileOutput#writeLines` can write them back out,
`FileOutput#write` / `append` / `writeLines` / `exists` / `delete`,
`Dir#mkdir` / `mkdirs` / `delete` / `copy` / `move`, and `Dir#exists` / `isFile` /
`isDirectory` / `list` / `listFull`; they are copied into NUL-terminated syscall
path buffers at runtime, with runtime directory listings exposed through the
same sorted runtime line-list representation. Direct
`Dir#current()` emits runtime `getcwd`, so generated native executables observe
their execution cwd rather than the cwd used during native build. `Dir#home()`
reads the generated executable's `HOME`, and `Dir#temp()` reads runtime `TMPDIR`
with `/tmp` as its Linux fallback.
`CommandLine#args()` returns the generated executable's process arguments
excluding argv[0] as the same runtime line-list representation, including
direct, unqualified, aliased-helper, and function-local native calls.
`Process#exit(code)` emits a native process exit after evaluating the code
argument, so generated native CLI tools can return explicit status codes.
`StandardInput#all()` / `stdin()` read stdin into a fixed-buffer runtime string,
and `StandardInput#lines()` / `stdinLines()` expose stdin as the same runtime
line-list representation used by native file and argv helpers.
`Environment#vars()` / `env()` return the generated executable's environment as
`KEY=VALUE` runtime line-list entries. `Environment#get(name)` / `getEnv(name)`
read a single variable value using static or runtime string keys, while
`Environment#exists(name)` / `hasEnv(name)` checks for one without failing when
it is absent.
`println(FileInput#all(path))` / `println(FileInput#readAll(path))` streams
runtime file content without requiring the file to exist during native build.
Immutable runtime `FileInput#all(path)` / `readAll(path)` bindings can also be
printed or concatenated through a fixed native string buffer, compared with
`==` / `!=` or `assertResult`, and queried with `isEmptyString`, `length`,
method-style `toString`, `substring` / `at` with static or runtime integer
indexes, ASCII-whitespace trimming, `repeat` with static or runtime integer
counts, ASCII case conversion, simple `matches` with static or runtime
patterns, first-occurrence `replace` with static or runtime literal operands,
all-occurrence `replaceAll`, UTF-8 `reverse`, `startsWith`, `endsWith`,
method-style `contains`, `indexOf`, and
`lastIndexOf`;
oversized results are reported as source-located runtime errors.
`FileInput#open` callback
folding preserves mutable callback effects when the callback's final value
remains statically recoverable. Static `null` is
supported for printing and
`Map#get` misses; `()` is supported for printing, static string concatenation,
ordinary equality, and `assertResult`. Static strings/lists/records/maps/sets/
null also support ordinary `==` / `!=` when both sides are statically known.
`ToDo()` emits the same `not implemented yet` runtime failure message from a
native executable. Native runtime failures for `assert`, `assertResult`,
`head([])`, negative `sleep`, and negative string helper indexes/counts include
source location prefixes on stderr. Native FileOutput syscall failures also
report source-located stderr diagnostics instead of continuing silently, as do
Dir copy/mkdir/delete/move failures.
Unsupported constructs fail at build time instead of falling back to the
evaluator.

## Quick Start

Create `hello.kl`:

```klassic
println("Hello, World!")
```

Run it:

```bash
cargo run -- hello.kl
```

## Syntax Examples

### Variables

```klassic
val one = 1

mutable i = 1
i = i + 1
i += 1
```

`val` bindings are immutable. `mutable` bindings can be reassigned.

### Functions

```klassic
val add = (x, y) => x + y

def fact(n) =
  if(n < 2) 1 else n * fact(n - 1)

println(add(1, 2))
println(fact(5))
```

### Blocks And Cleanup

```klassic
mutable i = 0
while(i < 10) {
  i += 1
} cleanup {
  println(i)
}
```

Cleanup clauses run after the associated expression finishes.

### Collections

```klassic
val list1 = [1, 2, 3]
val list2 = [
  1
  2
  3
]

val map = %["A": 1, "B": 2]

val set1 = %(1, 2, 3)
val set2 = %(
  1
  2
  3
)
```

Lists, maps, and sets accept commas, spaces, and line breaks as separators where
the language grammar allows collection separators.

### Strings

```klassic
val name = "Klassic"
println("Hello, #{name}!")
println(substring("abcdef", 1, 3))
```

### Records

```klassic
record Point {
  x: Int
  y: Int
}

val p = Point(10, 20)
println(p.x + p.y)

def add_xy(o) = o.x + o.y
println(add_xy(record { x = 1; y = 2 }))
```

Record field access participates in the Rust type checker's row-polymorphic
constraints, including nominal records used through structural field functions.
Call-site lambdas are checked against expected function types, so reducers such
as `foldLeft(xs)([])((acc, e) => e #cons acc)` infer the empty accumulator from
the reducer result instead of escaping through `*`.

### Modules And Imports

```klassic
module math.demo {
  def double(x) = x * 2
}

import math.demo.{double}
println(double(21))
```

### Type Classes

```klassic
typeclass Show<'a> where {
  show: ('a) => String
}

instance Show<Int> where {
  def show(x: Int): String = "Int: " + x
}

def display<'a>(x: 'a): String where Show<'a> = show(x)

println(display(42))
```

### Trust Surface

```klassic
axiom sortedBase(xs: List<Int>): { true }

theorem sortedAgain(xs: List<Int>): { true } =
  sortedBase(xs)
```

This compiles normally, warns under `--warn-trust`, and fails under
`--deny-trust` because it depends on an axiom.

## Repository Layout

- `src/`: native `klassic` CLI binary
- `crates/klassic-span`: source spans and diagnostics
- `crates/klassic-syntax`: lexer, parser, and AST
- `crates/klassic-rewrite`: placeholder desugaring and syntax normalization
- `crates/klassic-types`: type inference, records, type classes, and proof checks
- `crates/klassic-eval`: evaluator, runtime behavior, builtins, modules, and REPL state
- `crates/klassic-runtime`: shared runtime crate scaffold
- `crates/klassic-macro-peg`: Rust macro PEG implementation
- `tests/`: Rust integration tests and `.kl` golden harnesses
- `test-programs/`: sample Klassic programs used by the test harness
- `docs/`: architecture, implementation, and spec notes

## Development

Use `rg` for source search and keep changes covered by Rust tests:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
cargo test -p klassic-macro-peg
```

The main build/test path is Rust-only.
