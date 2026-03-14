#!/usr/bin/env bash
set -euo pipefail
echo "=== Learner Agent Setup ==="
command -v python3 &>/dev/null || { echo "ERROR: python3 required"; exit 1; }
if command -v cargo &>/dev/null; then
    echo "Building TUI dashboard..."
    (cd learner-tui && cargo build --release)
else
    echo "cargo not found — TUI won't be built (optional)"
fi
echo "Installing Python dependencies..."
pip3 install -r requirements.txt
[ -f config.yaml ] || { cp config.example.yaml config.yaml; echo "Created config.yaml — edit with your settings"; }
[ -f .env ] || { cp .env.example .env; echo "Created .env — add your API keys"; }
mkdir -p db logs
echo ""
echo "=== Setup Complete ==="
echo "  1. Edit .env with your API keys"
echo "  2. Edit config.yaml with your agents and data sources"
echo "  3. Run: ./learn --once    (single pass)"
echo "  4. Run: ./learn -p        (parallel mode)"
echo "  5. Run: ./learn --tui     (dashboard)"
