#!/bin/sh
set -euo pipefail

echo "=== TrajLens Dev Setup ==="

# Load environment (AWS keys, API keys for LLM testing)
if [ -f .env ]; then
    set -a && . .env && set +a
    echo "Loaded .env"
fi

# Claude Code
if ! command -v claude >/dev/null 2>&1; then
    echo "Installing Claude Code..."
    curl -fsSL https://claude.ai/install.sh | bash
else
    echo "Claude Code: installed"
fi

# uv + Python deps
if ! command -v uv >/dev/null 2>&1; then
    echo "Installing uv..."
    curl -LsSf https://astral.sh/uv/install.sh | sh
fi
uv sync --dev

# Rust (full features including LLM)
cargo build --features "cli,svg-rust,llm-bedrock"

# Git hooks
./dev_utils/install-hooks.sh

# Web viewer
if command -v npm >/dev/null 2>&1; then
    cd trajlens-web && npm install && cd ..
fi

echo ""
echo "=== Dev environment ready ==="
echo "  Run tests:  cargo test"
echo "  Analyze:    cargo run --features 'cli,svg-rust,llm-bedrock' --bin trajlens -- analyze <log> -o output/"
