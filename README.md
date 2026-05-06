# AgentSH

Make any Linux, macOS, or Windows terminal agentic — powered by a local LLM.
No cloud. No account. No API key.

## How it works

1. Type a natural language request in your terminal
2. AgentSH shows you exactly what commands it plans to run
3. You approve (or cancel)
4. Commands execute, output prints live
5. A short explanation tells you what was done and why

## Install

### One-liner (Linux / macOS)
curl -fsSL https://raw.githubusercontent.com/YOUR_USERNAME/agentsh/main/install/install.sh | bash

### Windows (PowerShell)
irm https://raw.githubusercontent.com/YOUR_USERNAME/agentsh/main/install/install.ps1 | iex

### Via cargo
cargo install agentsh

## Requirements
- [Ollama](https://ollama.com) running locally (install.sh handles this)
- 8 GB RAM minimum (for llama3.1:8b)
- Any terminal on Linux, macOS, or Windows

## Configuration
~/.agentsh/config.toml is created on first run.
Set a different model: agentsh --model qwen2.5-coder:14b

## Supported models (any Ollama model works)
| Model | RAM | Best for |
|---|---|---|
| llama3.1:8b | 8 GB | General tasks, fast |
| qwen2.5-coder:14b | 12 GB | Code tasks |
| mistral-nemo | 8 GB | Tool calling |
| deepseek-r1:14b | 12 GB | Complex reasoning |