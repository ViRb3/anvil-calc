#!/usr/bin/env sh
set -eu

wasm-pack build --release --target web --out-dir pkg
