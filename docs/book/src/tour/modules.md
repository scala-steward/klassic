# Modules and Imports

Klassic groups related definitions into modules. A module declares a
dotted name and brackets its contents in `{ ... }`.

```kl
module math.demo {
  def double(x) = x * 2
  def triple(x) = x * 3
}

import math.demo.{double, triple}
println(double(21))   // 42
println(triple(7))    // 21
```

## Selective import

Pick out exactly what you need:

```kl
import math.demo.{double}
double(10)
```

## Module aliases

Bind the whole module to a shorter name:

```kl
import math.demo as M
M.double(10)
M.triple(10)
```

## Aliasing builtin modules

The same syntax works for the language's own standard names:

```kl
import Map as M
import FileInput.{readAll}

val sizes = %["a": 1, "b": 2]
println(M.size(sizes))
```

## Nesting

Modules can nest:

```kl
module outer.inner {
  def f() = 42
}

println(outer.inner.f())
```
