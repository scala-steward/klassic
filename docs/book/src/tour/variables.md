# Variables and Values

Klassic has two kinds of bindings:

```kl
val pi = 3.14159        // immutable
mutable counter = 0     // reassignable
counter = counter + 1
counter += 1
```

`val` bindings cannot be reassigned. `mutable` bindings can.

## Type annotations

Klassic infers types Hindley-Milner style, so you rarely need
annotations. They are still allowed (and sometimes required at module
boundaries):

```kl
val name: String = "Klassic"
def square(n: Int): Int = n * n
```

## Built-in scalar types

| Type | Examples |
|---|---|
| `Int` | `0`, `42`, `-7`, `0xff` |
| `Long` | `42L`, `1_000_000_000L` |
| `Float` | `3.14F`, `1.0e9F` |
| `Double` | `3.14`, `1.0e9` |
| `Bool` | `true`, `false` |
| `String` | `"hello"`, `"interpolated #{x}"` |
| `Null` | `null` (singleton) |
| `Unit` | `()` (the only inhabitant) |

## Numeric literals

```kl
val byte = 0xff             // hex
val short = 256s            // 16-bit suffix
val long = 1_000L           // underscore separators allowed
val pi = 3.14F              // F → Float, no suffix → Double
```

## Operators

Standard arithmetic operators (`+`, `-`, `*`, `/`, `%`), comparison
(`==`, `!=`, `<`, `<=`, `>`, `>=`), and the boolean `&&` / `||` /
`!` work as you would expect. `&&` and `||` short-circuit.

```kl
val ok = (1 + 2) * 3 == 9 && true
println(ok)   // true
```
