#!/bin/sh
# Install git hooks for this project.
# Run once after cloning: ./dev_utils/install-hooks.sh

HOOK=.git/hooks/pre-commit

cat > "$HOOK" << 'HOOK_CONTENT'
#!/bin/sh
# Pre-commit hook: format Rust and Python files.
# Fails if formatting changes anything — forces you to stage the formatted version.

set -e

# Rust
if command -v cargo >/dev/null 2>&1; then
    if ! cargo fmt -- --check >/dev/null 2>&1; then
        cargo fmt
        echo "[pre-commit] Rust formatted. Re-stage and commit again."
        exit 1
    fi
fi

# Python (prefer ruff, fall back to black)
PY_DIRS="dev_utils trajlens-py"
if command -v ruff >/dev/null 2>&1; then
    if ! ruff format --check $PY_DIRS >/dev/null 2>&1; then
        ruff format $PY_DIRS
        echo "[pre-commit] Python formatted. Re-stage and commit again."
        exit 1
    fi
elif command -v black >/dev/null 2>&1; then
    if ! black --check --quiet $PY_DIRS 2>/dev/null; then
        black $PY_DIRS
        echo "[pre-commit] Python formatted. Re-stage and commit again."
        exit 1
    fi
fi
HOOK_CONTENT

chmod +x "$HOOK"
echo "Installed pre-commit hook at $HOOK"
