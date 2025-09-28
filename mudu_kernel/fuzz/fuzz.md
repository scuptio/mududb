# Installation

Install cargo-fuzz

```
cargo install cargo-fuzz

```

Install llvm-tools, for run coverage

```shell
rustup component add llvm-tools
```

Install llvm

```
apt install llvm
```

# Setting

Set LIBCLANG_PATH environment variable to a path where
['libclang.so', 'libclang-*.so'] can be found,

```
export LIBCLANG_PATH=/usr/lib/llvm-14/lib/
```

Install llvm-tools-preview tools,

```
rustup component add llvm-tools-preview
```

Otherwise, cargo fuzz coverage would complain errors:

```
Caused by:
   0: Failed to run command: "/[path_to_rust_lib]/bin/llvm-profdata" "merge" "-sparse" [XXX]
   
```

# Usage

1. run fuzz testing on target
    ```shell
    cargo +nightly fuzz run [target]
    ```

2. Minify target corpus of input files
    ```shell
    cargo +nightly fuzz cmin [target]
    ```

3. Generate test coverage(only run case in corpus)

   run fuzz coverage
   ```shell
    cargo +nightly fuzz coverage [target]
   ```

4. Generate golden corpus (in folder /[path_to_project]/fuzz/golden_corpus)
   ```shell
   GOLDEN_CORPUS=ture cargo +nightly fuzz coverage [target]
   ```

   
