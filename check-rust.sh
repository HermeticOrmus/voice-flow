#!/usr/bin/env bash
# check-rust.sh: the gate of the Rust workspace. The Python tests are the contract: the same
# test_voice_flow.py and test_engine.py run against the scripts (./check.sh) and against the Rust
# binaries (here, through VF_IMPL=rust). On a shared build host, run it inside your build-slot
# wrapper and under nice: the cargo steps use every core.
set -euo pipefail
cd "$(dirname "$0")"
export PYTHONDONTWRITEBYTECODE=1
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
for t in voice_flow engine hosts; do
    VF_IMPL=rust python3 -m unittest discover -s tests -p "test_$t.py"
done
# The whole Python suite last (the public-tree guard and the bash run of the same contract).
./check.sh
