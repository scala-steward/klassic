# Installing Klassic

Klassic builds with the standard Rust toolchain.

## Requirements

- A recent stable Rust (`rustc 1.85+` is comfortable; CI tracks the
  Rust 1.95 lints).
- Linux x86_64 if you want to use the native compiler. The evaluator
  runs anywhere Rust does.

## Build from source

```bash
git clone https://github.com/klassic/klassic.git
cd klassic
cargo build --release
```

The release binary lands at `./target/release/klassic`. Add it to your
`PATH` if you plan to use it routinely:

```bash
export PATH="$PWD/target/release:$PATH"
klassic --help
```

## Sanity check

```bash
klassic -e "1 + 2"
# 3
```

If that prints `3`, you have a working install.

## Run the test suite (optional)

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

The CI configuration in `.github/workflows/ci.yml` runs the same three
gates on every push.
