#!/bin/sh
set -euo pipefail

echo "=== TrajLens Installation ==="

# Rust toolchain
if ! command -v cargo >/dev/null 2>&1; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    . "$HOME/.cargo/env"
fi

# Build CLI
echo "Building trajlens CLI..."
cargo build --release --features "cli,svg-rust"
echo "Binary: target/release/trajlens"

# Python (uv for parser scripts)
if ! command -v uv >/dev/null 2>&1; then
    echo "Installing uv (Python package manager)..."
    curl -LsSf https://astral.sh/uv/install.sh | sh
fi
uv sync

# Git hooks
./dev_utils/install-hooks.sh

# Optional: web viewer
if command -v npm >/dev/null 2>&1; then
    echo "Installing web viewer dependencies..."
    cd trajlens-web && npm install && cd ..
fi

echo ""
echo "=== Done ==="
echo "  CLI:  ./target/release/trajlens analyze <log> -o output/"
echo "  Web:  cd trajlens-web && npm run dev"
echo "  LLM:  cargo build --release --features 'cli,svg-rust,llm-bedrock'"
