# Hello World

Save this as `hello.kl`:

```kl
println("Hello, World!")
```

You can run it three ways.

## Evaluate from the command line

```bash
klassic -e 'println("Hello, World!")'
# Hello, World!
```

`-e` evaluates a single expression without ever touching the
filesystem.

## Run the file through the evaluator

```bash
klassic hello.kl
# Hello, World!
```

`klassic <path>` and `klassic -f <path>` are equivalent — both run
the program through the evaluator.

## Compile to a native ELF

```bash
klassic build hello.kl -o hello
./hello
# Hello, World!
```

The native compiler emits a Linux x86_64 ELF that talks directly to
the kernel through syscalls. There is no `libc` dependency, so the
resulting binary is a few KiB and starts essentially instantly.

```bash
file hello
# hello: ELF 64-bit LSB executable, x86-64 ...

ls -lh hello
# -rwxr-xr-x 1 you you ~10K hello
```

## When does the evaluator beat the compiler?

The native compiler only supports a slice of the language — see
[Native Compiler Coverage](../reference/native-coverage.md) for the
full matrix. If you reach for a feature it doesn't yet handle, the
build emits a source-located diagnostic and exits non-zero. There is
no silent fallback to the evaluator. For exploratory work, run
through the evaluator (`klassic <path>`) or the [REPL](./repl.md).
