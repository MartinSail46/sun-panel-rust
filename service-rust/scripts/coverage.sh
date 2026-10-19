#!/usr/bin/env sh
set -eu

cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

# Coverage uses a separate target directory. Avoid large incremental caches and
# keep enough debug information for readable source reports.
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=1
export CARGO_PROFILE_TEST_DEBUG=1

mkdir -p target/coverage
cargo llvm-cov --locked --all-targets --no-report
cargo llvm-cov report --ignore-filename-regex '/tests/' --html --output-dir target/coverage
cargo llvm-cov report --ignore-filename-regex '/tests/' --lcov --output-path target/coverage/lcov.info
cargo llvm-cov report --ignore-filename-regex '/tests/' --json --output-path target/coverage/coverage.json
python3 scripts/coverage_summary.py target/coverage/lcov.info target/coverage/summary.json
