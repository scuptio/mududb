# Installation

Install cargo-fuzz

```
cargo install cargo-fuzz

```

Install llvm-tools to run coverage

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

Otherwise, `cargo fuzz coverage` would fail with errors:

```
Caused by:
   0: Failed to run command: "/[path_to_rust_lib]/bin/llvm-profdata" "merge" "-sparse" [XXX]
   
```

# Usage

1. run fuzz testing on target
    ```shell
    cargo +nightly-2026-06-17 fuzz run [target]
    ```

2. Minify target corpus of input files
    ```shell
    cargo +nightly-2026-06-17 fuzz cmin [target]
    ```

3. Generate test coverage(only run case in corpus)

   run fuzz coverage
   ```shell
    cargo +nightly-2026-06-17 fuzz coverage [target]
   ```

4. Generate golden corpus (in folder /[path_to_project]/fuzz/golden_corpus)
   ```shell
   GOLDEN_CORPUS=true cargo +nightly-2026-06-17 fuzz coverage [target]
   ```

# Full run + corpus management

`run_fuzz_all.py` drives every fuzz target end to end and keeps the corpora
healthy:

```shell
python3 mudu_kernel/fuzz/run_fuzz_all.py [-t SECONDS] [-j N] [--rss-limit-mb MB] [--targets a,b,c] [--with-coverage] [--list]
```

Memory: each libFuzzer worker is capped at `--rss-limit-mb` (default 2048
MB); with `-j N` peak fuzzing memory is roughly N times that. The build
itself is already capped by the workspace `.cargo/config.toml` (`jobs = 4`,
rust-lld wrapper).

Per target it:

1. Seeds `corpus/<target>/` from `golden_corpus/<target>/` (existing files
   are kept), so historical inputs take part in mutation.
2. Runs `cargo fuzz run <target> -- -max_total_time=<t> -use_value_profile=1`;
   libFuzzer writes newly-covered inputs into `corpus/<target>/`.
3. Stops with a non-zero exit code and prints a single-shot reproduction
   command if a new file appears in `artifacts/<target>/`.
4. Replays the corpus with `GOLDEN_CORPUS=1`, dumping every input
   (md5-named) into `golden_corpus/<target>/`; plain
   `cargo test -p mudu_kernel` replays them via `_test_target`.
5. Minimizes the corpus with `cargo fuzz cmin <target>`.

`--with-coverage` additionally renders HTML reports under
`fuzz/coverage/<target>/` (requires `llvm-tools-preview`; missing tools
downgrade the step to a warning).

Both `corpus/` and `golden_corpus/` are git-tracked. The script ends by
printing `git status --short` for them — commit the changes to keep the
grown corpus. The script itself never mutates git state.

Example (10 minutes per target, 4 parallel workers, 1 GB per worker):

```shell
python3 mudu_kernel/fuzz/run_fuzz_all.py -t 600 -j 4 --rss-limit-mb 1024 --with-coverage
```

   
