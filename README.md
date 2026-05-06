# AgentSH

> Make any Linux, macOS, or Windows terminal agentic — powered entirely by a local LLM.
> No cloud. No account. No API key. No data leaves your machine.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey)
![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange)
![CI](https://github.com/D9-cell/AGENTSH/actions/workflows/ci.yml/badge.svg)

---

## What is AgentSH?

AgentSH is a Rust-built CLI tool that wraps your existing terminal and makes it understand natural language. Type what you want in plain English — it silently plans the shell commands needed, shows you the full plan, asks for your permission, executes everything, and then explains what it did and why.

It works inside any terminal you already use (bash, zsh, fish, PowerShell), supports every major local LLM runtime, and **activates automatically every time you open a terminal** after install.

---

## How it works

```
~/projects ❯ set up a python venv and install flask and requests
```

**Step 1 — AgentSH plans silently.** A spinner appears while the local LLM works. You never see its reasoning — only the result.

```
  ⠸ Planning...
```

**Step 2 — You see the full plan and approve it.**

```
╭─────────────────── AgentSH Plan ───────────────────────╮
│                                                          │
│   1   python -m venv .venv                   ● SAFE     │
│   2   source .venv/bin/activate              ● SAFE     │
│   3   pip install flask requests             ● SAFE     │
│                                                          │
│   3 commands · estimated ~5s                            │
│                                                          │
│   [Y] Approve all    [N] Cancel    [F2] Allow all       │
╰──────────────────────────────────────────────────────────╯
```

**Step 3 — Commands execute. Output prints in bordered blocks.**

```
╭─ pip install flask requests ────────────── exit 0 · 3.1s ─╮
│  Collecting flask                                           │
│    Downloading flask-3.0.3-py3-none-any.whl (101 kB)       │
│  Successfully installed flask requests                      │
╰─────────────────────────────────────────────────────────────╯
```

**Step 4 — A short plain-English explanation follows.**

```
╭─ why ──────────────────────────────────────────────────────╮
│  💡 Created an isolated Python environment in .venv so     │
│     your dependencies don't conflict with system packages, │
│     then installed Flask and Requests into it.             │
╰────────────────────────────────────────────────────────────╯
```

Regular shell commands like `ls -la` or `git status` pass straight through with zero LLM involvement and zero latency. Their output also appears in bordered blocks showing exit code and elapsed time.

---

## Features

- **Natural language → shell commands** — describe any task in plain English
- **Inline autocomplete** — ghost-text suggestions as you type, sourced from your history and common command patterns. Press `→` or `Tab` to accept
- **Smart permission panel** — see every planned command with SAFE / HIGH / CRITICAL risk labels before anything runs
- **Full-permission session mode** — type `--allow-all` or press `F2` to auto-approve safe plans with a 2-second countdown. CRITICAL commands always require explicit confirmation
- **Bordered output blocks** — every command's output is wrapped with its exit code and elapsed time
- **Powerline-style prompt** — shows current directory, git branch, and dirty status
- **Startup banner** — styled ASCII art on every launch
- **Status bar** — live model, connection, and session mode at the bottom of the terminal
- **Auto-activates on terminal open** — after install, every new terminal starts AgentSH automatically
- **Multi-runtime LLM support** — works with Ollama, LM Studio, LocalAI, llama.cpp, and Jan
- **Smart install** — detects what you already have installed, never overwrites it

---

## Requirements

| Requirement | Notes |
|---|---|
| OS | Linux, macOS, or Windows |
| RAM | 8 GB minimum (for the default `llama3.1:8b` model) |
| Local LLM | Any supported runtime — the installer detects or installs one |
| Rust | Only needed if building from source |

---

## Installation

### Linux / macOS — one-liner

```bash
curl -fsSL https://raw.githubusercontent.com/D9-cell/AGENTSH/main/install/install.sh | bash
```

### Windows — PowerShell

```powershell
irm https://raw.githubusercontent.com/D9-cell/AGENTSH/main/install/install.ps1 | iex
```

### Manual binary (no Rust needed)

Download a pre-built binary from the [Releases page](https://github.com/D9-cell/AGENTSH/releases):

```bash
# Linux x86_64 example
chmod +x agentsh-linux-x86_64
sudo mv agentsh-linux-x86_64 /usr/local/bin/agentsh
```

### Build from source

```bash
git clone https://github.com/D9-cell/AGENTSH.git
cd AGENTSH
cargo build --release
sudo mv target/release/agentsh /usr/local/bin/agentsh
```

---

## What the installer does

The install script is not a simple downloader. It runs an interactive setup wizard:

**1 — Detects every local LLM runtime already on your machine.**

Runtimes it scans for: Ollama, LM Studio, LocalAI, llama.cpp server, and Jan. If you have any of them, it shows you a menu — it will not reinstall anything you already have.

```
Scanning for local LLM runtimes...
  ✓ Ollama      (detected)
  ✓ LM Studio   (detected)

Found 2 runtime(s). Choose one to use:

  [1] Ollama
  [2] LM Studio
  [3] Install Ollama (recommended)
  [4] Configure manually later

Enter choice [1]:
```

**2 — Lists models already available on your chosen runtime.**

```
Models available on Ollama:

  [1] llama3.1:8b
  [2] qwen2.5-coder:14b
  [3] Pull a new model...

Recommended: llama3.1:8b or qwen2.5-coder:14b
Enter choice [1]:
```

**3 — Writes your choice to `~/.agentsh/config.toml`** so AgentSH is configured for your exact setup.

**4 — Injects auto-activation into your shell** (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/conf.d/agentsh.fish`, or PowerShell `$PROFILE`). The injection is guarded — it will not fire in scripts, CI environments, IDE terminals, or nested shells.

**5 — Prints model recommendations** so you know what to pull next for better performance.

```
💡 Recommended models for AgentSH:
   Best overall:     deepseek-r1:8b      (ollama pull deepseek-r1:8b)
   Best for coding:  qwen2.5-coder:14b   (ollama pull qwen2.5-coder:14b)
   Fastest:          llama3.1:8b         (ollama pull llama3.1:8b)
   Low RAM (4GB):    phi3.5-mini         (ollama pull phi3.5-mini)
```

---

## Auto-activation

After install, AgentSH starts automatically whenever you open a new terminal. You do not need to type `agentsh` — it is just there.

The activation is smart. It does **not** fire in these cases:
- Non-interactive shells (scripts, CI, cron jobs, `ssh -c`)
- VS Code and JetBrains integrated terminals
- Any shell that is already running inside an AgentSH session (no nesting)
- If the binary is not found (e.g. after uninstall — your shell just opens normally)

**To disable auto-activation:**

```bash
agentsh deactivate
```

This cleanly removes the activation block from your rc file and prints what it changed. You can still run AgentSH manually with `agentsh` at any time.

---

## Usage

### Natural language requests

Just describe what you want. AgentSH handles the rest.

```
~/projects ❯ create a new git repo and make the first commit
~/projects ❯ find all log files older than 7 days and delete them
~/projects ❯ show me which process is using port 8080
~/projects ❯ compress the images folder into a zip file
~/projects ❯ install and start nginx
~/projects ❯ rename all .jpeg files in this folder to .jpg
```

### Direct shell commands — passthrough

Anything that looks like a shell command is passed straight to your system shell. No LLM, no delay.

```
~/projects ❯ ls -la
~/projects ❯ git status
~/projects ❯ cd ../other-project
~/projects ❯ docker ps
~/projects ❯ vim config.yaml
```

### Inline autocomplete

As you type, a greyed-out suggestion appears based on your command history and common patterns:

```
~/projects ❯ git sta▌tus --short
```

- Press `→` or `Tab` to accept the full suggestion
- Press `Escape` to dismiss it
- Press `Enter` to submit what you have typed, ignoring the suggestion

### Approving plans

When a natural language request produces a plan:

- Press **`y`** or **Enter** — approve all commands and execute
- Press **`n`** — cancel, nothing runs

Risk labels on every command:

| Label | Colour | Examples |
|---|---|---|
| `● SAFE` | green | `mkdir`, `pip install`, `git clone` |
| `● HIGH` | yellow | `sudo`, `chmod 777`, `curl \| bash` |
| `● CRITICAL` | red | `rm -rf`, `dd if=`, `mkfs.` |

CRITICAL commands highlight the entire row in dark red and are **never** auto-approved regardless of session mode.

### Full-permission mode (mission mode)

For longer tasks where you want AgentSH to run a full sequence without stopping to ask after each plan, enable full-permission mode:

```
~/projects ❯ --allow-all
```

Or press **`F2`** at any time. The prompt changes to show you are in this mode:

```
~/projects [AUTO] ❯
```

In this mode:
- Plans are still shown so you can see what will run
- Each plan auto-approves after a **2-second countdown** — press `n` to cancel during the countdown
- CRITICAL-scored commands still require explicit `y`
- Press **`Ctrl-C`** to stop the current mission and return to normal per-plan approval mode

You can also enable it at launch:

```bash
agentsh --allow-all
```

---

## CLI reference

```bash
# Start AgentSH
agentsh

# Session options
agentsh --model qwen2.5-coder:14b               # override model for this session
agentsh --base-url http://192.168.1.5:11434/v1  # use Ollama on another machine
agentsh --allow-all                              # start in full-permission mode

# Information
agentsh --version                                # print version
agentsh --config                                 # print path to active config file

# Setup subcommands
agentsh setup                                    # re-run the first-time setup wizard
agentsh select-model                             # switch your default model interactively
agentsh deactivate                               # remove auto-activation from your shell rc
```

---

## Configuration

`~/.agentsh/config.toml` is written automatically during install. Edit it at any time.

```toml
[llm]
base_url     = "http://localhost:11434/v1"  # your LLM runtime endpoint
model        = "llama3.1:8b"               # default model
timeout_secs = 120                          # max seconds to wait for LLM response

[safety]
require_confirm   = true   # always ask before executing (strongly recommended)
auto_approve_safe = false  # set true to skip confirm for SAFE-only plans

[agent]
max_commands_per_turn = 8   # LLM will not plan more than this many commands at once
context_lines         = 40  # lines of recent terminal output sent to LLM as context
```

**Precedence:** hardcoded defaults → `config.toml` → CLI flags. CLI flags always win for the current session only.

---

## Supported LLM runtimes

AgentSH works with any runtime that exposes an OpenAI-compatible `/v1/chat/completions` endpoint.

| Runtime | Default port | Notes |
|---|---|---|
| [Ollama](https://ollama.com) | 11434 | Recommended — easiest setup |
| [LM Studio](https://lmstudio.ai) | 1234 | GUI model manager |
| [LocalAI](https://localai.io) | 8080 | Self-hosted, Docker-friendly |
| [llama.cpp server](https://github.com/ggerganov/llama.cpp) | 8080 | Minimal, low overhead |
| [Jan](https://jan.ai) | 1337 | Desktop app with built-in models |

To point AgentSH at a different runtime, set `base_url` in your config or use `--base-url` at launch.

---

## Supported models

Any model available on your chosen runtime works. These are recommended for AgentSH specifically:

| Model | RAM | Best for |
|---|---|---|
| `deepseek-r1:8b` | 8 GB | Best reasoning, complex multi-step tasks |
| `qwen2.5-coder:14b` | 12 GB | Best for coding, scripting, file editing |
| `llama3.1:8b` | 8 GB | Fast, general tasks — **default** |
| `mistral-nemo` | 8 GB | Strong tool-calling accuracy |
| `phi3.5-mini` | 4 GB | Low-RAM machines |

To switch your default model interactively:

```bash
agentsh select-model
```

To pull a new model with Ollama:

```bash
ollama pull deepseek-r1:8b
```

---

## Switching models after install

You do not need to reinstall to change models. Run:

```bash
agentsh select-model
```

This shows all models currently available on your runtime, lets you pick one or pull a new one, and updates `~/.agentsh/config.toml` immediately. The change takes effect next time you open a terminal.

---

## Running tests

```bash
cargo test
```

The test suite covers the command classifier, safety pattern matching, history-backed autocomplete engine, and the agent loop against a mocked LLM server.

---

## Troubleshooting

**AgentSH is not starting automatically when I open a terminal**

Run `agentsh` manually once. If it works, the activation line may not have been injected yet. Re-run the installer or add it manually:

```bash
# Add to ~/.bashrc or ~/.zshrc
if [ -t 1 ] && [ -z "$AGENTSH_ACTIVE" ] && [ "$TERM_PROGRAM" != "vscode" ] && command -v agentsh > /dev/null 2>&1; then
  exec agentsh
fi
```

Then open a new terminal.

**AgentSH is starting inside my VS Code terminal and I do not want that**

This should not happen — the activation line checks `$TERM_PROGRAM` and skips VS Code terminals. If it is still firing, run `agentsh deactivate` and add the activation line back manually without the `exec` keyword so AgentSH runs alongside rather than replacing your shell session.

**`agentsh` command not found after install**

The binary may not be on your PATH. Check:

```bash
ls /usr/local/bin/agentsh      # Linux/macOS
which agentsh
```

If missing, re-run the installer or move the binary manually:

```bash
sudo mv ./agentsh /usr/local/bin/agentsh
chmod +x /usr/local/bin/agentsh
```

**Cannot connect to LLM runtime**

```bash
# For Ollama
ollama serve       # start manually
ollama list        # confirm your model is downloaded

# For other runtimes — check their respective start commands
# and verify base_url in ~/.agentsh/config.toml matches the running port
```

**LLM responses are slow**

Switch to a smaller or faster model. `llama3.1:8b` is the best balance of speed and quality for most tasks. `phi3.5-mini` works on 4 GB RAM. Raise `timeout_secs` in your config if requests time out.

**I want to use Ollama running on another machine**

```bash
agentsh --base-url http://192.168.1.10:11434/v1
```

Or set `base_url` in `~/.agentsh/config.toml` to make it permanent.

**I want to uninstall AgentSH**

```bash
agentsh deactivate             # remove auto-activation
sudo rm /usr/local/bin/agentsh # remove binary
rm -rf ~/.agentsh              # remove config and history
```

---

## Contributing

Open an issue before starting significant work so we can align on the approach.

```bash
git clone https://github.com/D9-cell/AGENTSH.git
cd AGENTSH
cargo clippy -- -D warnings   # must pass with zero warnings
cargo test                     # must pass on your platform
cargo build --release          # verify the binary builds cleanly
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide.

---

## License

MIT — see [LICENSE](LICENSE).

---

*Built with Rust · Runs entirely on your machine · No cloud required*
