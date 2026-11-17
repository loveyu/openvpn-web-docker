#!/usr/bin/env bash
# 统一格式化仓库内的源代码、配置和文档；所有工具失败时立即停止。
set -Eeuo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "$0")/.." && pwd)
PRETTIER="$ROOT_DIR/frontend/node_modules/.bin/prettier"

cd "$ROOT_DIR"

cargo fmt --all
ruff format scripts
ruff check --fix scripts

shfmt -w -i 4 -ci -sr docker/*.sh scripts/*.sh

RUST_LOG=error taplo fmt Cargo.toml pyproject.toml rust-toolchain.toml taplo.toml

"$PRETTIER" --write --config "$ROOT_DIR/.prettierrc.json" \
    "$ROOT_DIR/README.md" \
    "$ROOT_DIR/docs/**/*.md" \
    "$ROOT_DIR/.github/**/*.yml" \
    "$ROOT_DIR"/*.json \
    "$ROOT_DIR"/*.yaml \
    "$ROOT_DIR/scripts/*.mjs" \
    "$ROOT_DIR/frontend/**/*.{css,html,js,json,ts,tsx}"
