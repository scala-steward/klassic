# Strings

Klassic strings are UTF-8. There are two flavours that interoperate
freely in the evaluator and through native compilation:

- **Static strings** — string literals and the result of pure folding.
  They live in the executable's `.data` section.
- **Heap strings** — produced by `__gc_string*` builtins (and, on the
  native compiler's roadmap, by every dynamic operation eventually).
  They live on the GC heap and survive arbitrary collections.

Both kinds work with `println` and the standard library helpers below.
The difference is mostly internal — heap strings can grow at runtime
without filling fixed-size scratch buffers.

## Literals and interpolation

```kl
val name = "Klassic"
val greeting = "Hello, #{name}!"
println(greeting)   // Hello, Klassic!
```

`#{...}` evaluates an arbitrary expression and splices its rendering
into the surrounding string.

## Concatenation

```kl
val parts = "foo" + "bar"
val mixed = "count = " + 42
```

## Common operations

| Function | Behaviour |
|---|---|
| `length(s)` | Byte length |
| `substring(s, i, j)` | Bytes `[i, j)` |
| `at(s, i)` | One-byte string at index |
| `trim(s)`, `trimLeft(s)`, `trimRight(s)` | Strip ASCII whitespace |
| `toLowerCase(s)`, `toUpperCase(s)` | ASCII case shift |
| `replace(s, from, to)` | Replace first occurrence |
| `replaceAll(s, from, to)` | Replace every occurrence |
| `startsWith(s, prefix)`, `endsWith(s, suffix)` | Boolean predicates |
| `contains(s, needle)`, `indexOf(s, needle)` | Membership / first index |
| `repeat(s, n)` | Concatenate `s` with itself `n` times |
| `reverse(s)` | UTF-8 aware reverse |
| `split(s, delimiter)` | Returns a list of segments |
| `join(parts, separator)` | Inverse of `split` |

Method-style calls work too:

```kl
val tidy = "  Klassic  ".trim().toUpperCase()
println(tidy)   // KLASSIC
```

## Heap strings

When you need string values that grow at runtime (concatenating in a
loop, building output incrementally), reach for the GC builtins:

```kl
mutable greeting = __gc_string("Hello")
greeting = __gc_string_concat(greeting, __gc_string(", world!"))
println(greeting)
```

See [Heap-Allocated Strings](../gc/strings.md) for the full toolkit.
