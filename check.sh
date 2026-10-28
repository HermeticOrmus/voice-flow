#!/usr/bin/env bash
# check.sh: the voice-flow gate for its Python parts. uv supplies the listener's packages.
set -euo pipefail
cd "$(dirname "$0")"
export PYTHONDONTWRITEBYTECODE=1
for f in bin/voice-ideas; do [ -f "$f" ] && uv run --quiet --python 3.12 --with onnxruntime --with numpy python -c "import ast,sys; ast.parse(open('$f').read())" || true; done
uv run --quiet --python 3.12 --with onnxruntime --with numpy python -m unittest discover -s tests -v
