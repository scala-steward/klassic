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
constraints.

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
cargo test
cargo test -p klassic-macro-peg
```

The main build/test path is Rust-only.
