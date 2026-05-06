#!/usr/bin/env bash
set -euo pipefail

echo "==> Checking for Ollama..."
if ! command -v ollama &>/dev/null; then
  echo "==> Installing Ollama..."
  curl -fsSL https://ollama.com/install.sh | sh
fi

MODEL="${AGENTSH_MODEL:-llama3.1:8b}"
echo "==> Pulling model: $MODEL"
ollama pull "$MODEL"

echo "==> Installing agentsh..."
if command -v cargo &>/dev/null; then
  cargo install agentsh
else
  OS=$(uname -s | tr '[:upper:]' '[:lower:]')
  ARCH=$(uname -m)
  URL="https://github.com/YOUR_USERNAME/agentsh/releases/latest/download/agentsh-${OS}-${ARCH}"
  curl -fsSL "$URL" -o /usr/local/bin/agentsh
  chmod +x /usr/local/bin/agentsh
fi

echo ""
echo "Done! Run: agentsh"