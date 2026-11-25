# Per-file coverage checks

`cargo make coverage` writes the merged Rust report to
`target/llvm-cov/html/index.html` and its underlying data to
`target/llvm-cov/coverage.json`.

List every file whose function, line or region coverage does not **strictly
exceed** 80%:

```sh
python3 script/coverage/check_file_rates.py target/llvm-cov/coverage.json
```

The command writes `target/coverage-audit/audit.md` and `audit.json`, and
exits with status 1 while any file is pending. It compares covered/total
counts without rounding; exactly 80% does not pass. Metrics with no
instrumented items are not applicable. Test files, generated files and
benchmark files present in the input report remain in the audit.

To track improvements, save the original export **outside** `target/llvm-cov`
before rerunning coverage (the runner cleans that directory), then compare:

```sh
cp target/llvm-cov/coverage.json target/coverage-baseline.json
# Add tests and run cargo make coverage again.
python3 script/coverage/check_file_rates.py target/llvm-cov/coverage.json \
  --baseline target/coverage-baseline.json
```

Baseline files missing from the new report remain failures, rather than
counting as improvements. Use complete merged exports for this comparison;
a package-only run omits other packages and cannot establish repository-wide
completion. Focused runs can validate individual files during development.

The default runner currently does not enable `testing/socket`. Tests behind
that feature, external-service tests and ignored benchmarks need separate
execution and instrumentation before their coverage can be assessed. A
test reporting success after skipping an unavailable dependency does not
prove that dependency's behavior was exercised.

Audit utility regression tests:

```sh
python3 -m unittest discover -s script/coverage -p 'test_*.py'
```
