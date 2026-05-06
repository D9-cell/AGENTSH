# AgentSH

> Make any Linux, macOS, or Windows terminal agentic — powered entirely by a local LLM.
> No cloud. No account. No API key. No data leaves your machine.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey)
![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange)
![CI](https://github.com/D9-cell/AGENTSH/actions/workflows/ci.yml/badge.svg)

---

## What is AgentSH?

AgentSH is a Rust-built CLI tool that wraps your existing terminal and makes it understand natural language. You describe what you want in plain English — it figures out the exact shell commands needed, shows them to you before running anything, asks for your permission, executes them, and then explains what it did and why.

It works inside any terminal you already use (bash, zsh, PowerShell) and runs entirely on your own machine using [Ollama](https://ollama.com).

---

## How it works

You type a natural language request:

```
~/projects ❯ set up a python venv and install flask and requests
```

**Step 1 — AgentSH plans silently.** The local LLM works out what commands are needed. You never see its reasoning.

**Step 2 — You see the full plan and approve it.**

```
╭─ AgentSH plan ──────────────────────────── Approve all? [Y/n] ─╮
│  1.   python -m venv .venv                            SAFE      │
│  2.   source .venv/bin/activate                       SAFE      │
│  3.   pip install flask requests                      SAFE      │
╰────────────────────────────────────────────────────────────────╯
```

**Step 3 — Commands execute. Output prints live.**

```
Collecting flask
  Downloading flask-3.0.3-py3-none-any.whl (101 kB)
Successfully installed flask requests
```

**Step 4 — A short plain-English explanation follows.**

```
╭─ why ──────────────────────────────────────────────────────────╮
│  Created an isolated Python environment so your project's      │
│  dependencies don't conflict with system packages, then        │
│  installed Flask and Requests into it.                         │
╰────────────────────────────────────────────────────────────────╯
```

Regular shell commands like `ls -la` or `git status` pass straight through — no LLM involved, zero latency.

---

## Requirements

| Requirement | Notes |
|---|---|
| OS | Linux, macOS, or Windows |
| RAM | 8 GB minimum (for the default `llama3.1:8b` model) |
| Ollama | Installed and running locally — the install script handles this |
| Rust | Only needed if you install via `cargo install` |

---

## Installation

### Linux / macOS — one-liner

```bash
curl -fsSL https://raw.githubusercontent.com/D9-cell/AGENTSH/main/install/install.sh | bash
```

This will automatically install Ollama if missing, pull the default model, and place the `agentsh` binary on your PATH.

### Windows — PowerShell

```powershell
irm https://raw.githubusercontent.com/D9-cell/AGENTSH/main/install/install.ps1 | iex
```

### Via Cargo

```bash
cargo install agentsh
```

### Manual binary (no Rust needed)

Download a pre-built binary for your platform from the [Releases page](https://github.com/D9-cell/AGENTSH/releases), then:

```bash
# Linux example
chmod +x agentsh-linux-x86_64
sudo mv agentsh-linux-x86_64 /usr/local/bin/agentsh
```

---

## First run

```bash
agentsh
```

On first launch AgentSH creates `~/.agentsh/config.toml` with defaults, sets up the session history database, confirms Ollama is reachable, and drops you into the prompt:

```
~/your/directory ❯
```

You are ready. Type anything in plain English.

---

## Usage

### Natural language requests

```
~/projects ❯ create a new git repo and make the first commit
~/projects ❯ find all log files older than 7 days and delete them
~/projects ❯ show me which process is using port 8080
~/projects ❯ compress the images folder into a zip file
~/projects ❯ install and start nginx
```

### Direct shell commands — passthrough

Anything that looks like a shell command is sent straight to your shell with no LLM call at all.

```
~/projects ❯ ls -la
~/projects ❯ git status
~/projects ❯ cd ../other-project
~/projects ❯ docker ps
```

AgentSH detects direct commands by checking whether the first word is a known binary, whether the input starts with a path (`./`, `/`, `~`), or whether it contains shell metacharacters. Everything else is treated as natural language.

### Approving and cancelling plans

- Press **Enter** or type **`y`** — approve and execute all commands
- Type **`n`** — cancel, nothing runs

Dangerous commands are always labelled so you can make an informed decision:

| Label | Colour | Examples |
|---|---|---|
| SAFE | green | `mkdir`, `pip install`, `git clone` |
| HIGH | yellow | `sudo`, `chmod 777`, `curl \| bash` |
| CRITICAL | red | `rm -rf`, `dd if=`, `mkfs.` |

### CLI flags

```bash
agentsh                                          # start with config defaults
agentsh --model qwen2.5-coder:14b               # use a different model this session
agentsh --base-url http://192.168.1.5:11434/v1  # remote Ollama instance
agentsh --version                                # print version
agentsh --config                                 # print path to config file
agentsh setup                                    # run the first-time setup wizard
```

---

## Configuration

`~/.agentsh/config.toml` is created automatically on first run. Edit it with any text editor.

```toml
[llm]
base_url     = "http://localhost:11434/v1"  # Ollama endpoint
model        = "llama3.1:8b"               # default model
timeout_secs = 120                          # max seconds to wait for LLM

[safety]
require_confirm   = true   # always ask before executing (strongly recommended)
auto_approve_safe = false  # set true to skip confirm for SAFE-only plans

[agent]
max_commands_per_turn = 8   # LLM will not plan more than this many commands
context_lines         = 40  # lines of recent output sent to LLM as context
```

CLI flags override config values for the current session only.

---

## Supported models

Any model available in Ollama works. Recommended options:

| Model | RAM | Best for |
|---|---|---|
| `llama3.1:8b` | 8 GB | General tasks, fast — **default** |
| `qwen2.5-coder:14b` | 12 GB | Coding, scripting, file editing |
| `mistral-nemo` | 8 GB | Strong tool-calling accuracy |
| `deepseek-r1:14b` | 12 GB | Complex multi-step tasks |
| `phi3.5-mini` | 4 GB | Low-RAM machines |

To pull a model:

```bash
ollama pull qwen2.5-coder:14b
```

To use it permanently, set `model` in `~/.agentsh/config.toml`. To use it for one session:

```bash
agentsh --model qwen2.5-coder:14b
```

---

## Building from source

```bash
git clone https://github.com/D9-cell/AGENTSH.git
cd AGENTSH
cargo build --release
./target/release/agentsh
```

To build a fully static binary on Linux:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

---

## Running tests

```bash
cargo test
```

The test suite covers the command classifier, all safety patterns, and the agent loop using a mocked Ollama server.

---

## Troubleshooting

**Cannot connect to Ollama**

```bash
ollama serve     # start Ollama manually
ollama list      # confirm your model is downloaded
```

**LLM responses are slow**

Switch to a smaller model like `phi3.5-mini` for faster responses on lower-spec hardware. Raise `timeout_secs` in your config if requests time out before completing.

**Using a remote Ollama instance**

```bash
agentsh --base-url http://192.168.1.10:11434/v1
```

Or set `base_url` in `~/.agentsh/config.toml` to make it permanent.

---

## Contributing

Open an issue before starting significant work so we can align on the approach.

```bash
git clone https://github.com/D9-cell/AGENTSH.git
cd AGENTSH
cargo clippy   # must pass with zero warnings
cargo test     # must pass on your platform
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide.

---

## License

MIT — see [LICENSE](LICENSE).

---

*Built with Rust · Powered by Ollama · Runs entirely on your machine*
