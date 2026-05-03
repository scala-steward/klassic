# Control Flow

## `if` expressions

`if` is an expression — every branch must return the same type.

```kl
val parity = if (n % 2 == 0) "even" else "odd"
```

Without an `else`, the value is `Unit`:

```kl
if (debug) println("hello")
```

## Ternary operator

`if` doubles as a ternary. There is no separate `?:` syntax.

## `while` loops

```kl
mutable i = 0
while (i < 10) {
  println(i)
  i += 1
}
```

`while` is an expression that returns `Unit`.

## `foreach`

```kl
foreach (x in [1, 2, 3]) {
  println(x)
}

foreach (entry in %["alice": 30, "bob": 27]) {
  println(entry)
}
```

The native compiler unrolls `foreach` over static integer lists, so
small loops have zero per-iteration overhead.

## Blocks as expressions

Curly braces make a block. The last expression in the block becomes
the value of the block.

```kl
val score = {
  val raw = compute()
  val clamped = if (raw < 0) 0 else raw
  clamped
}
```

This pattern is handy for keeping local helpers tucked next to where
they are used.

## Match-on-shape

Klassic does not yet have an algebraic `match` expression. Use chained
`if` / `else` for pattern-style dispatch, or method-style calls on
records.
