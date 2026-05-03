# Functions

Klassic has two function syntaxes that differ mostly in style.

## Named definitions

```kl
def add(x, y) = x + y

def fact(n) =
  if (n < 2) 1
  else n * fact(n - 1)

println(add(1, 2))     // 3
println(fact(5))       // 120
```

`def` is the preferred form for top-level functions. It infers
parameter and return types when it can, and you can still annotate:

```kl
def repeat(s: String, n: Int): String =
  if (n == 0) ""
  else s + repeat(s, n - 1)
```

## Lambda expressions

```kl
val add = (x, y) => x + y
val inc = (n) => n + 1
println(add(2, 3))     // 5
```

## Placeholder syntax

Underscore placeholders create one-off lambdas:

```kl
val inc = _ + 1
val mul3 = 3 * _
println(inc(10))       // 11
```

## First-class functions

Functions are values. Pass them around freely:

```kl
def apply2(f, x) = f(f(x))
println(apply2(_ + 1, 10))   // 12
```

## Closures

Lambdas capture their enclosing bindings:

```kl
def make_counter() = {
  mutable n = 0
  () => {
    n += 1
    n
  }
}

val tick = make_counter()
println(tick())       // 1
println(tick())       // 2
println(tick())       // 3
```

## Currying

Multi-parameter `def`s can be partially applied through curried
helpers like `assertResult`, `cons`, `map`, and `foldLeft`. See
[Native Compiler Coverage](../reference/native-coverage.md) for
which curried shapes the native compiler folds.
