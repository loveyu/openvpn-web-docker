#!/usr/bin/env bash
# 以零容忍模式执行全部语言和容器配置检查；任何 warning 或格式偏差都失败。
set -Eeuo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "$0")/.." && pwd)
FRONTEND_BIN="$ROOT_DIR/frontend/node_modules/.bin"

require_command() {
    command_name=$1
    if ! command -v "$command_name" > /dev/null 2>&1; then
        echo "缺少质量检查工具: $command_name" >&2
        exit 127
    fi
}

for command_name in cargo ruff shfmt shellcheck yamllint taplo hadolint docker; do
    require_command "$command_name"
done

cd "$ROOT_DIR"

git diff --check

# Rust：编译器、Clippy 和 rustdoc 的 warning 全部提升为错误。
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings" cargo clippy --all-targets --all-features --locked -- -D warnings
RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings" cargo test --all-features --locked

# TypeScript/React：类型、Oxlint 规则和 Prettier 均不允许 warning。
if [ ! -x "$FRONTEND_BIN/tsc" ] || [ ! -x "$FRONTEND_BIN/oxlint" ] || [ ! -x "$FRONTEND_BIN/prettier" ]; then
    echo "缺少前端质量检查依赖，请先执行 pnpm --dir frontend install --frozen-lockfile。" >&2
    exit 127
fi
(cd "$ROOT_DIR/frontend" && "$FRONTEND_BIN/tsc" --noEmit)
(cd "$ROOT_DIR/frontend" && "$FRONTEND_BIN/oxlint" -D correctness -D suspicious --deny-warnings .)
"$FRONTEND_BIN/prettier" --check --config "$ROOT_DIR/.prettierrc.json" \
    "$ROOT_DIR/README.md" \
    "$ROOT_DIR/docs/**/*.md" \
    "$ROOT_DIR/.github/**/*.yml" \
    "$ROOT_DIR"/*.json \
    "$ROOT_DIR"/*.yaml \
    "$ROOT_DIR/frontend/**/*.{css,html,js,json,ts,tsx}"
python3 -W error -c \
    'import xml.etree.ElementTree as ET; ET.parse("frontend/public/favicon.svg")'
(cd "$ROOT_DIR/frontend" && "$FRONTEND_BIN/tsc" -b)
(cd "$ROOT_DIR/frontend" && "$FRONTEND_BIN/vite" build)

# Python：Ruff 格式、静态规则和字节码编译全部严格执行。
ruff format --check scripts
ruff check scripts
python3 -W error -m compileall -q scripts

# POSIX Shell：格式差异和 ShellCheck 的所有级别均失败。
shfmt -d -i 4 -ci -sr docker/*.sh scripts/*.sh
shellcheck --shell=sh --severity=style docker/*.sh
shellcheck --shell=bash --severity=style scripts/*.sh

# YAML、TOML、Dockerfile 和 Compose 配置。
yamllint --strict .github compose.yaml
RUST_LOG=error taplo fmt --check Cargo.toml pyproject.toml rust-toolchain.toml taplo.toml
hadolint --failure-threshold warning Dockerfile
docker compose config --quiet
