# Type Classes

Klassic supports type classes with method dispatch resolved by the
type checker. Higher-kinded examples work too.

## Defining a class

```kl
typeclass Show<'a> where {
  show: ('a) => String
}
```

`'a` is a type variable. The class declares one method, `show`, of
the given signature.

## Implementing instances

```kl
instance Show<Int> where {
  def show(x: Int): String = "Int: " + x
}

instance Show<Bool> where {
  def show(b: Bool): String = if (b) "yes" else "no"
}
```

## Constrained polymorphism

Functions can require a class constraint with `where`:

```kl
def display<'a>(x: 'a): String where Show<'a> = show(x)

println(display(42))      // Int: 42
println(display(true))    // yes
```

When the type checker sees `display(42)`, it picks `Show<Int>` from the
available instances and inlines the right `show` implementation.

## Higher-kinded constraints

Type variables of kind `* -> *` work too:

```kl
typeclass Functor<'f> where {
  fmap: ('a) => ('b) => ('f<'a>) => 'f<'b>
}
```

You can use `instance Functor<List>` to declare how `fmap` lifts a
function over a list.

## When to reach for type classes

- You want one name (`show`, `fmap`, `eq`) to resolve differently
  depending on the type at the call site.
- You want compile-time errors when a type lacks an instance.
- Records with fixed fields are simpler — only escalate to type
  classes when overloading really helps.
