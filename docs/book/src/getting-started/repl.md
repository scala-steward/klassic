# The REPL

Run `klassic` with no arguments to enter the read-eval-print loop:

```text
$ klassic
> 1 + 2
3
> val x = 10
> x * 4
40
> def double(n) = n * 2
> double(21)
42
> :history
1: 1 + 2
2: val x = 10
3: x * 4
4: def double(n) = n * 2
5: double(21)
> :exit
```

## Commands

| Command | Action |
|---|---|
| `:history` | Show every line you have entered this session. |
| `:exit` | Leave the REPL. `Ctrl-D` on an empty line works too. |

## Multi-line input

Long expressions wrap naturally — the REPL keeps reading until braces
and brackets close:

```text
> def fact(n) =
.   if (n < 2) 1
.   else n * fact(n - 1)
> fact(5)
120
```

## Persisted state

REPL state is per-process; closing the REPL throws away every binding.
For longer experiments, write the code to a `.kl` file and reload it.
