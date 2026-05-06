# AgentSH — V3 Changes Build Prompt

> This file describes every change, fix, and new feature to implement on top of the existing AgentSH Rust codebase at https://github.com/D9-cell/AGENTSH
> Read this entire document before touching any code. Implement changes in the order listed in Section 9.
> This version adds two new major features on top of V2: smart local LLM detection and selection during install, and auto-activation of AgentSH whenever the user opens any terminal.

---

## 0. Critical bug to fix first — install script broken

### Problem
The install script at `install/install.sh` currently runs:
```bash
cargo install agentsh
```
This fails with `error: could not find 'agentsh' in registry 'crates-io'` because the package is not published to crates.io yet. Additionally, the old script blindly installs Ollama and pulls a model without first checking whether the user already has a working local LLM setup.

---

## 0A. New feature — smart LLM detection and selection during install

### Behaviour the install script must implement

The install process must detect every local LLM runtime already present on the machine before installing or pulling anything. If one or more are found, it presents an interactive menu letting the user pick which to use. It never assumes Ollama is the only option.

**Runtimes to detect (check for their CLI binary in PATH):**

| Runtime | Detection command | API base URL |
|---|---|---|
| Ollama | `ollama` | `http://localhost:11434/v1` |
| LM Studio | `lms` | `http://localhost:1234/v1` |
| LocalAI | `local-ai` | `http://localhost:8080/v1` |
| llama.cpp server | `llama-server` or `llama.cpp` | `http://localhost:8080/v1` |
| Jan | `jan` | `http://localhost:1337/v1` |

**Detection flow:**

```
Step 1: Scan for all installed runtimes.
Step 2: If none found → go to "No LLM found" path (Section 0A-2).
Step 3: If one or more found → go to "LLMs found" path (Section 0A-1).
Step 4: After runtime is chosen → list available models on that runtime.
Step 5: User selects a model OR accepts the recommended default.
Step 6: Write chosen runtime base_url and model into ~/.agentsh/config.toml.
```

### 0A-1 — "LLMs found" path

When at least one runtime is detected, display:

```
╔══════════════════════════════════════════════╗
║         AgentSH — LLM Setup                 ║
╚══════════════════════════════════════════════╝

Found the following local LLM runtimes on your system:

  [1] Ollama          (detected)
  [2] LM Studio       (detected)
  [3] Install Ollama  (recommended — broadest model support)
  [4] Install nothing — I'll configure manually later

Enter your choice [1]:
```

Rules:
- Only show detected runtimes in the numbered list. Always append "Install Ollama" and "Configure manually" as the last two options regardless.
- Default (pressing Enter with no input) = first detected runtime.
- If the user chooses an already-detected runtime, do NOT install anything new. Skip straight to model selection.
- If the user chooses "Install Ollama", run the Ollama installer then continue to model selection with Ollama.
- If the user chooses "Configure manually", print instructions for setting `base_url` and `model` in `~/.agentsh/config.toml` and exit the LLM setup section.

After runtime is selected, **list available models on that runtime**:

For Ollama:
```bash
MODELS=$(ollama list 2>/dev/null | tail -n +2 | awk '{print $1}')
```

For LM Studio:
```bash
MODELS=$(lms ls 2>/dev/null | grep -oP '[\w.\-]+(?=\s)' | head -20)
```

For other runtimes: attempt `curl -s http://localhost:{PORT}/v1/models` and parse the `id` fields from the JSON response.

Display the model list:

```
Models available on Ollama:

  [1] llama3.1:8b          (currently loaded)
  [2] qwen2.5-coder:14b
  [3] mistral-nemo
  [4] deepseek-r1:14b
  [5] Pull a new model...
  [6] Keep current config

Recommended for AgentSH: llama3.1:8b or qwen2.5-coder:14b
Enter your choice [1]:
```

Rules:
- If the user picks an existing model, write it to config and continue.
- If the user picks "Pull a new model...", show a sub-prompt:
  ```
  Enter model name to pull (e.g. deepseek-r1:8b): _
  ```
  Run `ollama pull <name>` then write to config.
- After model selection, display a recommendation notice:
  ```
  ✓ Selected: llama3.1:8b via Ollama

  💡 Recommended models for best results with AgentSH:
     • deepseek-r1:8b     — best reasoning, great for complex tasks
     • qwen2.5-coder:14b  — best for coding and file editing
     • mistral-nemo        — fastest tool-calling
     Pull any of these later with: ollama pull <model-name>
  ```

### 0A-2 — "No LLM found" path

When no local LLM runtime is detected:

```
╔══════════════════════════════════════════════╗
║         AgentSH — LLM Setup                 ║
╚══════════════════════════════════════════════╝

No local LLM runtime detected on this machine.

AgentSH requires a local LLM to function. Choose one to install:

  [1] Ollama          (recommended — easiest setup, widest model support)
  [2] LM Studio       (GUI app with model manager — good for beginners)
  [3] LocalAI         (self-hosted, OpenAI-compatible server)
  [4] Skip — I'll install manually and configure later

Enter your choice [1]:
```

- Option 1: run `curl -fsSL https://ollama.com/install.sh | sh`, then pull the recommended model.
- Option 2: print `Download LM Studio from: https://lmstudio.ai` and open the URL if `xdg-open` is available. Then pause and wait for user to press Enter before continuing.
- Option 3: run the LocalAI install instructions from `https://localai.io/basics/getting_started/`.
- Option 4: print config instructions and continue without an LLM configured.

After any install, continue to model selection the same as Section 0A-1.

### 0A-3 — Recommendation notice always shown

Regardless of which path was taken, always print this before finishing the LLM setup:

```
💡 Model recommendations for AgentSH:
   Best overall:     deepseek-r1:8b      (ollama pull deepseek-r1:8b)
   Best for coding:  qwen2.5-coder:14b   (ollama pull qwen2.5-coder:14b)
   Fastest:          llama3.1:8b         (ollama pull llama3.1:8b)
   Low RAM (4GB):    phi3.5-mini         (ollama pull phi3.5-mini)
```

### Fix — rewrite `install/install.sh` completely

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO="https://github.com/D9-cell/AGENTSH"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="$HOME/.agentsh"
CONFIG_FILE="$CONFIG_DIR/config.toml"

# ── Colour helpers ──────────────────────────────────────────────
GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'
BOLD='\033[1m'; RESET='\033[0m'

header() { echo -e "\n${CYAN}${BOLD}╔══════════════════════════════════════════════╗"; \
           echo -e "║         AgentSH — LLM Setup                 ║"; \
           echo -e "╚══════════════════════════════════════════════╝${RESET}\n"; }

# ── Detect installed runtimes ───────────────────────────────────
DETECTED=()
declare -A RUNTIME_URL
detect_runtime() {
  local name=$1 cmd=$2 url=$3
  if command -v "$cmd" &>/dev/null; then
    DETECTED+=("$name")
    RUNTIME_URL["$name"]="$url"
    echo -e "  ${GREEN}✓${RESET} $name (detected)"
  fi
}

header
echo "Scanning for local LLM runtimes..."
detect_runtime "Ollama"     "ollama"       "http://localhost:11434/v1"
detect_runtime "LM Studio"  "lms"          "http://localhost:1234/v1"
detect_runtime "LocalAI"    "local-ai"     "http://localhost:8080/v1"
detect_runtime "llama.cpp"  "llama-server" "http://localhost:8080/v1"
detect_runtime "Jan"        "jan"          "http://localhost:1337/v1"

CHOSEN_RUNTIME=""
CHOSEN_URL=""

# ── Runtime selection ───────────────────────────────────────────
if [ ${#DETECTED[@]} -eq 0 ]; then
  echo -e "\n${YELLOW}No local LLM runtime detected.${RESET}"
  echo -e "Choose one to install:\n"
  echo "  [1] Ollama       (recommended)"
  echo "  [2] LM Studio    (GUI, good for beginners)"
  echo "  [3] LocalAI      (self-hosted server)"
  echo "  [4] Skip — configure manually later"
  echo -n "Enter choice [1]: "; read -r CHOICE; CHOICE=${CHOICE:-1}
  case "$CHOICE" in
    1) curl -fsSL https://ollama.com/install.sh | sh; CHOSEN_RUNTIME="Ollama"; CHOSEN_URL="http://localhost:11434/v1" ;;
    2) echo "Download LM Studio: https://lmstudio.ai"; command -v xdg-open &>/dev/null && xdg-open "https://lmstudio.ai"; echo "Press Enter after installing..."; read -r; CHOSEN_RUNTIME="LM Studio"; CHOSEN_URL="http://localhost:1234/v1" ;;
    3) echo "See: https://localai.io/basics/getting_started/"; CHOSEN_RUNTIME="LocalAI"; CHOSEN_URL="http://localhost:8080/v1" ;;
    4) echo "Set base_url and model in ~/.agentsh/config.toml after install."; CHOSEN_RUNTIME="manual"; CHOSEN_URL="http://localhost:11434/v1" ;;
  esac
else
  echo -e "\nFound ${#DETECTED[@]} runtime(s). Choose one to use:\n"
  IDX=1
  for R in "${DETECTED[@]}"; do echo "  [$IDX] $R"; ((IDX++)); done
  echo "  [$IDX] Install Ollama (recommended)"
  ((IDX++))
  echo "  [$IDX] Configure manually later"
  echo -n "Enter choice [1]: "; read -r CHOICE; CHOICE=${CHOICE:-1}
  DET_COUNT=${#DETECTED[@]}
  if [ "$CHOICE" -le "$DET_COUNT" ] 2>/dev/null; then
    CHOSEN_RUNTIME="${DETECTED[$((CHOICE-1))]}"
    CHOSEN_URL="${RUNTIME_URL[$CHOSEN_RUNTIME]}"
  elif [ "$CHOICE" -eq "$((DET_COUNT+1))" ]; then
    curl -fsSL https://ollama.com/install.sh | sh; CHOSEN_RUNTIME="Ollama"; CHOSEN_URL="http://localhost:11434/v1"
  else
    echo "Set base_url and model in ~/.agentsh/config.toml after install."; CHOSEN_RUNTIME="manual"; CHOSEN_URL="http://localhost:11434/v1"
  fi
fi

# ── Model selection ─────────────────────────────────────────────
CHOSEN_MODEL="llama3.1:8b"
if [ "$CHOSEN_RUNTIME" = "Ollama" ] && command -v ollama &>/dev/null; then
  echo -e "\nModels available on Ollama:"
  MODELS=(); IDX=1
  while IFS= read -r line; do
    MODEL_NAME=$(echo "$line" | awk '{print $1}')
    [ -z "$MODEL_NAME" ] && continue
    MODELS+=("$MODEL_NAME"); echo "  [$IDX] $MODEL_NAME"; ((IDX++))
  done < <(ollama list 2>/dev/null | tail -n +2)
  echo "  [$IDX] Pull a new model..."
  echo -e "\nRecommended: llama3.1:8b or qwen2.5-coder:14b"
  echo -n "Enter choice [1]: "; read -r MCHOICE; MCHOICE=${MCHOICE:-1}
  if [ "$MCHOICE" -eq "$IDX" ] 2>/dev/null; then
    echo -n "Model name to pull: "; read -r NEW_MODEL
    ollama pull "$NEW_MODEL"; CHOSEN_MODEL="$NEW_MODEL"
  elif [ "$MCHOICE" -le "${#MODELS[@]}" ] 2>/dev/null; then
    CHOSEN_MODEL="${MODELS[$((MCHOICE-1))]}"
  fi
fi

# ── Write config ────────────────────────────────────────────────
mkdir -p "$CONFIG_DIR"
cat > "$CONFIG_FILE" <<TOML
[llm]
base_url     = "$CHOSEN_URL"
model        = "$CHOSEN_MODEL"
timeout_secs = 120

[safety]
require_confirm   = true
auto_approve_safe = false

[agent]
max_commands_per_turn = 8
context_lines         = 40
TOML

echo -e "\n${GREEN}✓${RESET} Config written to $CONFIG_FILE"
echo -e "\n💡 Recommended models for AgentSH:"
echo "   Best overall:     deepseek-r1:8b      (ollama pull deepseek-r1:8b)"
echo "   Best for coding:  qwen2.5-coder:14b   (ollama pull qwen2.5-coder:14b)"
echo "   Fastest:          llama3.1:8b         (ollama pull llama3.1:8b)"
echo "   Low RAM (4GB):    phi3.5-mini         (ollama pull phi3.5-mini)"

# ── Install agentsh binary ──────────────────────────────────────
echo -e "\n==> Installing agentsh binary..."
if command -v cargo &>/dev/null; then
  TMP=$(mktemp -d)
  git clone --depth 1 "$REPO" "$TMP/agentsh"
  cd "$TMP/agentsh"
  cargo build --release
  sudo mv target/release/agentsh "$INSTALL_DIR/agentsh"
  cd ~; rm -rf "$TMP"
else
  OS=$(uname -s | tr '[:upper:]' '[:lower:]')
  ARCH=$(uname -m)
  case "$ARCH" in
    x86_64) ARCH="x86_64" ;; aarch64|arm64) ARCH="aarch64" ;;
    *) echo "Unsupported arch: $ARCH"; exit 1 ;;
  esac
  URL="$REPO/releases/latest/download/agentsh-${OS}-${ARCH}"
  curl -fsSL "$URL" -o /tmp/agentsh
  chmod +x /tmp/agentsh
  sudo mv /tmp/agentsh "$INSTALL_DIR/agentsh"
fi

# ── Auto-activation: inject into shell rc files ─────────────────
SHELL_NAME=$(basename "$SHELL")
RC_LINE='# AgentSH auto-activation'$'\n''[ -t 1 ] && [ -x "$(command -v agentsh)" ] && exec agentsh'

activate_shell() {
  local RC="$1"
  if [ -f "$RC" ] && ! grep -q "AgentSH auto-activation" "$RC"; then
    echo "" >> "$RC"
    echo "$RC_LINE" >> "$RC"
    echo -e "  ${GREEN}✓${RESET} Added auto-activation to $RC"
  fi
}

echo -e "\n==> Configuring auto-activation..."
case "$SHELL_NAME" in
  bash) activate_shell "$HOME/.bashrc"; activate_shell "$HOME/.bash_profile" ;;
  zsh)  activate_shell "$HOME/.zshrc" ;;
  fish) mkdir -p "$HOME/.config/fish/conf.d"
        echo "if status is-interactive; and command -v agentsh > /dev/null; exec agentsh; end" \
          > "$HOME/.config/fish/conf.d/agentsh.fish"
        echo -e "  ${GREEN}✓${RESET} Added auto-activation to fish config" ;;
  *)    echo "  Unknown shell '$SHELL_NAME'. Add this line to your shell rc file manually:"
        echo "  $RC_LINE" ;;
esac

echo ""
echo -e "${GREEN}${BOLD}✓ AgentSH installed and configured!${RESET}"
echo "  Open a new terminal and AgentSH will start automatically."
echo "  Or run now: agentsh"
echo ""
```

### Fix — rewrite `install/install.ps1` completely

```powershell
$ErrorActionPreference = "Stop"
$Repo = "https://github.com/D9-cell/AGENTSH"
$ConfigDir = "$env:USERPROFILE\.agentsh"
$ConfigFile = "$ConfigDir\config.toml"
$InstallDir = "$env:USERPROFILE\.local\bin"

function Write-Header {
    Write-Host "`n╔══════════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "║         AgentSH — LLM Setup                 ║" -ForegroundColor Cyan
    Write-Host "╚══════════════════════════════════════════════╝`n" -ForegroundColor Cyan
}

# ── Detect runtimes ─────────────────────────────────────────────
Write-Header
Write-Host "Scanning for local LLM runtimes..."

$runtimes = @()
$runtimeUrls = @{}

$checks = @(
    @{Name="Ollama";    Cmd="ollama";       Url="http://localhost:11434/v1"},
    @{Name="LM Studio"; Cmd="lms";          Url="http://localhost:1234/v1"},
    @{Name="LocalAI";   Cmd="local-ai";     Url="http://localhost:8080/v1"},
    @{Name="Jan";       Cmd="jan";          Url="http://localhost:1337/v1"}
)
foreach ($c in $checks) {
    if (Get-Command $c.Cmd -ErrorAction SilentlyContinue) {
        $runtimes += $c.Name
        $runtimeUrls[$c.Name] = $c.Url
        Write-Host "  ✓ $($c.Name) (detected)" -ForegroundColor Green
    }
}

$chosenRuntime = ""; $chosenUrl = ""; $chosenModel = "llama3.1:8b"

# ── Runtime selection ───────────────────────────────────────────
if ($runtimes.Count -eq 0) {
    Write-Host "`nNo local LLM runtime detected. Choose one to install:`n"
    Write-Host "  [1] Ollama       (recommended)"; Write-Host "  [2] LM Studio    (GUI)"
    Write-Host "  [3] Skip — configure manually later"
    $choice = Read-Host "Enter choice [1]"; if (-not $choice) { $choice = "1" }
    switch ($choice) {
        "1" { winget install Ollama.Ollama --silent --accept-source-agreements --accept-package-agreements; Start-Sleep 8; $chosenRuntime = "Ollama"; $chosenUrl = "http://localhost:11434/v1" }
        "2" { Start-Process "https://lmstudio.ai"; Read-Host "Press Enter after installing LM Studio"; $chosenRuntime = "LM Studio"; $chosenUrl = "http://localhost:1234/v1" }
        default { $chosenRuntime = "manual"; $chosenUrl = "http://localhost:11434/v1" }
    }
} else {
    Write-Host "`nFound $($runtimes.Count) runtime(s). Choose one:`n"
    for ($i = 0; $i -lt $runtimes.Count; $i++) { Write-Host "  [$($i+1)] $($runtimes[$i])" }
    Write-Host "  [$($runtimes.Count+1)] Install Ollama (recommended)"
    Write-Host "  [$($runtimes.Count+2)] Configure manually"
    $choice = Read-Host "Enter choice [1]"; if (-not $choice) { $choice = "1" }
    $idx = [int]$choice - 1
    if ($idx -ge 0 -and $idx -lt $runtimes.Count) {
        $chosenRuntime = $runtimes[$idx]; $chosenUrl = $runtimeUrls[$chosenRuntime]
    } elseif ([int]$choice -eq $runtimes.Count + 1) {
        winget install Ollama.Ollama --silent --accept-source-agreements --accept-package-agreements
        Start-Sleep 8; $chosenRuntime = "Ollama"; $chosenUrl = "http://localhost:11434/v1"
    } else { $chosenRuntime = "manual"; $chosenUrl = "http://localhost:11434/v1" }
}

# ── Model selection for Ollama ──────────────────────────────────
if ($chosenRuntime -eq "Ollama" -and (Get-Command ollama -ErrorAction SilentlyContinue)) {
    Write-Host "`nModels available on Ollama:"
    $models = @(); $idx = 1
    ollama list 2>$null | Select-Object -Skip 1 | ForEach-Object {
        $m = ($_ -split '\s+')[0]; if ($m) { $models += $m; Write-Host "  [$idx] $m"; $idx++ }
    }
    Write-Host "  [$idx] Pull a new model..."
    Write-Host "`nRecommended: llama3.1:8b or qwen2.5-coder:14b"
    $mc = Read-Host "Enter choice [1]"; if (-not $mc) { $mc = "1" }
    if ([int]$mc -eq $idx) {
        $newModel = Read-Host "Model name to pull"; ollama pull $newModel; $chosenModel = $newModel
    } elseif ([int]$mc -ge 1 -and [int]$mc -le $models.Count) { $chosenModel = $models[[int]$mc - 1] }
}

# ── Write config ────────────────────────────────────────────────
New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null
@"
[llm]
base_url     = "$chosenUrl"
model        = "$chosenModel"
timeout_secs = 120

[safety]
require_confirm   = true
auto_approve_safe = false

[agent]
max_commands_per_turn = 8
context_lines         = 40
"@ | Set-Content $ConfigFile
Write-Host "`n✓ Config written to $ConfigFile" -ForegroundColor Green

Write-Host "`n💡 Recommended models:"
Write-Host "   deepseek-r1:8b     — best reasoning  (ollama pull deepseek-r1:8b)"
Write-Host "   qwen2.5-coder:14b  — best coding     (ollama pull qwen2.5-coder:14b)"
Write-Host "   llama3.1:8b        — fastest         (ollama pull llama3.1:8b)"

# ── Install binary ──────────────────────────────────────────────
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    $tmp = [System.IO.Path]::GetTempPath() + [System.Guid]::NewGuid()
    New-Item -ItemType Directory -Path $tmp | Out-Null
    git clone --depth 1 $Repo "$tmp\agentsh"
    Push-Location "$tmp\agentsh"; cargo build --release
    Copy-Item "target\release\agentsh.exe" "$InstallDir\agentsh.exe" -Force
    Pop-Location; Remove-Item $tmp -Recurse -Force
} else {
    $url = "$Repo/releases/latest/download/agentsh-windows-x86_64.exe"
    Invoke-WebRequest $url -OutFile "$InstallDir\agentsh.exe"
}

# Add to PATH
$curPath = [Environment]::GetEnvironmentVariable("PATH","User")
if ($curPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("PATH","$curPath;$InstallDir","User")
}

# ── Auto-activation for PowerShell ─────────────────────────────
$psProfile = $PROFILE.CurrentUserCurrentHost
if (-not (Test-Path $psProfile)) { New-Item -Force -Path $psProfile | Out-Null }
$activationLine = 'if ($Host.UI.RawUI -and (Get-Command agentsh -ErrorAction SilentlyContinue)) { agentsh }'
if (-not (Select-String -Path $psProfile -Pattern "agentsh" -Quiet)) {
    Add-Content $psProfile "`n# AgentSH auto-activation`n$activationLine"
    Write-Host "✓ Added auto-activation to PowerShell profile" -ForegroundColor Green
}

Write-Host "`n✓ AgentSH installed! Open a new terminal — it will start automatically." -ForegroundColor Green
```

---

## 0B. New feature — auto-activation on terminal open

### What this means

After install, every time the user opens a new terminal window or tab, AgentSH must start automatically — the user should never need to type `agentsh` manually. The terminal just opens and is immediately agentic.

The install scripts in Section 0 already inject the activation line into shell rc files. This section defines what that activation line must do, how it must behave, and what the Rust code must do to support it correctly.

### Activation line behaviour rules

The shell rc activation line must satisfy ALL of the following:

1. **Only activate in interactive terminals.** Never activate when the shell is being used non-interactively (scripts, CI, cron, `ssh -c`, piped commands). Use `[ -t 1 ]` on bash/zsh and `status is-interactive` on fish.

2. **Only activate if the binary exists.** Use `command -v agentsh` to check. If the binary is missing, the shell must open normally without error.

3. **Do not activate inside an already-active AgentSH session.** AgentSH must export `AGENTSH_ACTIVE=1` when it starts. The activation line checks for this and skips if set, preventing infinite nesting when the user opens a subshell.

4. **Do not activate inside VS Code or JetBrains integrated terminals** unless the user opted in. Detect via `$TERM_PROGRAM` — if it equals `vscode` or `jetbrains`, skip activation.

**Final activation line for bash/zsh:**

```bash
# AgentSH auto-activation
if [ -t 1 ] && \
   [ -z "$AGENTSH_ACTIVE" ] && \
   [ "$TERM_PROGRAM" != "vscode" ] && \
   [ "$TERM_PROGRAM" != "jetbrains" ] && \
   command -v agentsh > /dev/null 2>&1; then
  exec agentsh
fi
```

**For fish shell (`~/.config/fish/conf.d/agentsh.fish`):**

```fish
if status is-interactive
    and test -z "$AGENTSH_ACTIVE"
    and test "$TERM_PROGRAM" != "vscode"
    and command -v agentsh > /dev/null 2>&1
    exec agentsh
end
```

**For PowerShell (`$PROFILE`):**

```powershell
# AgentSH auto-activation
if ([Environment]::GetEnvironmentVariable("AGENTSH_ACTIVE") -ne "1" -and
    $env:TERM_PROGRAM -ne "vscode" -and
    (Get-Command agentsh -ErrorAction SilentlyContinue)) {
    agentsh
}
```

### Changes required in `src/main.rs`

At the very start of `main()`, before anything else runs, export the guard variable:

```rust
std::env::set_var("AGENTSH_ACTIVE", "1");
```

This propagates to all child processes spawned by AgentSH and prevents re-entrant activation.

### New subcommand: `agentsh deactivate`

Add a `deactivate` subcommand via `clap`. When run:

1. Detects the user's shell from `$SHELL`.
2. Reads the relevant rc file(s).
3. Removes the AgentSH activation block (the comment line and the `if` block that follows it) using string search and removal.
4. Writes the file back.
5. Prints:
   ```
   ✓ Removed auto-activation from ~/.bashrc
     AgentSH will no longer start automatically.
     You can still run it manually with: agentsh
   ```

### New subcommand: `agentsh select-model`

Add a `select-model` subcommand that re-runs the model selection flow from Section 0A-1 interactively, without reinstalling. Lets users switch models at any time:

```
$ agentsh select-model

Models available on Ollama:
  [1] llama3.1:8b   (current)
  [2] qwen2.5-coder:14b
  [3] deepseek-r1:14b
  [4] Pull a new model...

Recommended: deepseek-r1:8b (best reasoning) · qwen2.5-coder:14b (best coding)
Enter choice [1]:
```

After selection, update `~/.agentsh/config.toml` with the new `model` value. Change takes effect on next AgentSH start.

---

## 1. New feature — auto-suggest commands as user types (inline completion)

### What it should look like

As the user types in the AgentSH prompt, a greyed-out inline suggestion appears to the right of the cursor, exactly like fish shell or zsh-autosuggestions:

```
~/projects ❯ git sta▌tus --short          ← greyed out suggestion
```

Pressing `→` (right arrow) or `Tab` accepts the full suggestion. Pressing `End` also accepts. Any other key dismisses it.

### How to implement

**New file: `src/suggest.rs`**

```rust
pub struct Suggester {
    history: Vec<String>,
    builtin_completions: HashMap<String, Vec<String>>,
}

impl Suggester {
    pub fn new(history: Vec<String>) -> Self { ... }

    /// Given what the user has typed so far, return the best single-line suggestion
    /// or None if no match. Never calls the LLM — must be instant (<1ms).
    pub fn suggest(&self, input: &str) -> Option<String> { ... }
}
```

**Suggestion sources, checked in this priority order:**

1. **History match** — scan `~/.agentsh/history.db` for the most recently used command that starts with the current input. Return the full command.
2. **Builtin completions** — a static `HashMap` of common command prefixes to their most common completions. Examples:
   - `"git "` → `["status", "add -A", "commit -m \"\"", "push", "pull", "log --oneline", "diff", "checkout -b"]`
   - `"docker "` → `["ps", "images", "run -it", "build -t", "compose up", "compose down"]`
   - `"cargo "` → `["build", "run", "test", "clippy", "fmt", "add"]`
   - `"npm "` → `["install", "run dev", "run build", "start", "test"]`
   - `"ls "` → `["-la", "-lh", "--color=auto"]`
3. **Filesystem completion** — if the input ends with a partial path, complete it using `std::fs::read_dir`. For example: `cat src/ma` → `cat src/main.rs`.
4. **No suggestion** → return `None`, render nothing.

**Integration into `src/repl.rs`:**

The readline loop must switch from a simple `std::io::stdin().read_line()` call to a raw-mode character-by-character reader using `crossterm`. On each keypress:
- Update the input buffer
- Call `suggester.suggest(&input_buffer)`
- If `Some(suggestion)`: print the typed portion in white, the remainder of the suggestion in dim grey (e.g. `\x1b[2m`), then move cursor back to after the typed portion
- On `→` / `Tab` / `End`: accept suggestion (copy to input buffer, print in white, move cursor to end)
- On `Escape`: clear suggestion
- On `Enter`: submit input as-is (do not auto-accept suggestion)
- On `Backspace`: update buffer, re-render suggestion

Do not use any external readline crate (rustyline, reedline) — implement this directly in `crossterm` raw mode. This keeps dependencies minimal and gives full control over rendering.

---

## 2. New feature — full-permission session mode (yolo mode)

### User-facing behaviour

When the user types `--allow-all` or presses `F2` at any point during a session, AgentSH enters **full-permission mode** for the rest of that session:

- A persistent status indicator appears in the prompt showing the mode is active:
  ```
  ~/projects [AUTO] ❯
  ```
- The permission panel is still shown (so the user can see what will run) but it auto-approves after a 2-second countdown. The user can still press `n` during the countdown to cancel.
- The agent runs all commands in a plan without pausing between them.
- The mode persists until `Ctrl-C` is pressed (which exits the current mission), after which the prompt returns to normal per-command permission mode.
- CRITICAL-scored commands are **never** auto-approved even in full-permission mode — they always require explicit `y`.

### How to implement

**Add to `src/context.rs`:**

```rust
pub enum PermissionMode {
    /// Default: ask before every plan
    PerPlan,
    /// Full session: auto-approve after countdown, except CRITICAL commands
    AutoApprove { countdown_secs: u8 },
}
```

Store `permission_mode: PermissionMode` on the `Context` struct.

**Add to `src/prompt_ui.rs`:**

```rust
/// Shows the permission panel with an auto-countdown if mode is AutoApprove.
/// Returns true if approved, false if cancelled.
pub fn show_permission_panel(
    commands: &[PlannedCommand],
    mode: &PermissionMode,
) -> bool {
    // render the panel as before
    // if AutoApprove:
    //   - show "Auto-approving in 2s... press N to cancel"
    //   - use crossterm to count down, refreshing the line each second
    //   - if user presses N during countdown: return false
    //   - if CRITICAL commands present: ignore countdown, require explicit y
    // if PerPlan:
    //   - show "Approve all? [Y/n]" and wait for input
}
```

**Activation:**

In `src/repl.rs`, detect `--allow-all` as a special input (not sent to the LLM) and switch `context.permission_mode`. Also bind `F2` key in the raw-mode reader.

**Mission mode (Ctrl-C handling):**

When the user asks a natural language request in AutoApprove mode, the agent runs the full multi-command plan as a "mission". If `Ctrl-C` is received during a mission:
- Stop the currently running subprocess immediately (send SIGTERM/SIGKILL on Unix, TerminateProcess on Windows)
- Print `Mission cancelled.`
- Reset `permission_mode` to `PerPlan`
- Return to prompt

---

## 3. New feature — cool terminal UI and visual polish

The goal is to make AgentSH look like a premium terminal tool — not a plain text REPL. All rendering must use `crossterm` and `ratatui`. No external fonts required. Must work in any terminal that supports 256-colour or truecolour.

### 3.1 Startup banner

On launch, display a one-time banner before the first prompt:

```
  ╔══════════════════════════════════════════╗
  ║   █████╗  ██████╗ ███████╗███╗   ██╗████████╗███████╗██╗  ██╗  ║
  ║  ██╔══██╗██╔════╝ ██╔════╝████╗  ██║╚══██╔══╝██╔════╝██║  ██║  ║
  ║  ███████║██║  ███╗█████╗  ██╔██╗ ██║   ██║   ███████╗███████║  ║
  ║  ██╔══██║██║   ██║██╔══╝  ██║╚██╗██║   ██║   ╚════██║██╔══██║  ║
  ║  ██║  ██║╚██████╔╝███████╗██║ ╚████║   ██║   ███████║██║  ██║  ║
  ╚══════════════════════════════════════════╝
         Agentic terminal · powered by local LLM
         Model: llama3.1:8b  ·  Ollama: connected ✓
         Type naturally or use shell commands directly.
         --allow-all  to enable full-permission mode
```

Render the banner using `crossterm` with gradient colouring (cyan → blue across the ASCII art). Keep the banner under 8 lines tall. Skip the banner if stdout is not a TTY (i.e. if piped).

### 3.2 Prompt design

Replace the plain prompt with a two-segment powerline-style prompt:

```
 ~/projects/agentsh  main ±  ❯ 
```

Segments (left to right):
- **Path segment**: dark background (`#1e2127`), text = shortened CWD (replace `$HOME` with `~`, truncate middle if > 40 chars, show only last 2 path components if longer). Colour: bright white on dark grey.
- **Git segment** (only when inside a git repo): show current branch name + status indicator. Colour: green on dark if clean, yellow on dark if dirty (uncommitted changes). Get branch via `git rev-parse --abbrev-ref HEAD` and status via `git status --porcelain`. Run these as subprocesses, timeout after 300ms, skip segment on timeout.
- **Arrow** `❯`: gradient magenta/purple.
- **In AutoApprove mode**: insert `[AUTO]` segment in amber between git and arrow.

Redraw the prompt after every command completes.

### 3.3 Command output blocks

Wrap the output of each executed command in a subtle bordered block, exactly like Warp Terminal's block concept:

```
╭─ git status ──────────────────────── exit 0 · 0.3s ─╮
│ On branch main                                        │
│ nothing to commit, working tree clean                 │
╰───────────────────────────────────────────────────────╯
```

- Border colour: dim cyan for exit code 0, dim red for any non-zero exit code.
- Header shows the command that was run (truncated to 40 chars if needed).
- Footer shows exit code and elapsed time in seconds (1 decimal place).
- Content is the raw stdout+stderr from the command, printed as-is inside the border.
- Block width matches the terminal width (`crossterm::terminal::size()`).
- If the output is longer than 40 lines, collapse it with a `... (N more lines) ...` indicator. The user can scroll with arrow keys to see the rest (implement a simple pager for long output).

### 3.4 Permission panel visual upgrade

The existing text-based panel becomes a proper `ratatui` widget:

```
╭─────────────────── AgentSH Plan ───────────────────────╮
│                                                          │
│   1   python -m venv .venv                   ● SAFE     │
│   2   source .venv/bin/activate              ● SAFE     │
│   3   pip install flask requests             ● SAFE     │
│                                                          │
│   3 commands · estimated ~8s                            │
│                                                          │
│   [Y] Approve all    [N] Cancel    [F2] Allow all       │
╰──────────────────────────────────────────────────────────╯
```

- SAFE bullets: green `●`
- HIGH bullets: yellow `●`  
- CRITICAL bullets: red `●` + the full row background turns dark red
- Show an estimated time (rough heuristic: 2s base + 1s per command)
- Show `[F2] Allow all` hint so users discover the full-permission mode

### 3.5 Explanation panel visual upgrade

```
╭─ why ──────────────────────────────────────────────────╮
│  💡 Created an isolated Python environment in .venv    │
│     so dependencies don't conflict with system         │
│     packages, then installed Flask and Requests.       │
╰────────────────────────────────────────────────────────╯
```

- Border colour: dim purple
- Add `💡` emoji prefix to the explanation text
- Word-wrap explanation at terminal width minus 6

### 3.6 Status bar (bottom of terminal, optional)

If the terminal is tall enough (>24 rows), render a single-line status bar pinned to the bottom:

```
 agentsh  v0.1.0   llama3.1:8b   Ollama ✓   ~/projects   [NORMAL]  
```

Use `crossterm::cursor` to position this at the last row. Update it after every command. In AutoApprove mode, change `[NORMAL]` to `[AUTO]` in amber.

---

## 4. New feature — natural language generates AND previews commands before executing

When the user types a natural language request, before showing the permission panel, show a brief "thinking" indicator (not the LLM's reasoning — just a spinner):

```
~/projects ❯ set up flask project
  ⠋ Planning...
```

Use `crossterm` to render a spinning braille character (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) that animates at 100ms intervals while the LLM call is in progress. Clear the spinner line completely when the plan arrives.

Implement this in `src/prompt_ui.rs`:

```rust
pub struct Spinner {
    handle: JoinHandle<()>,
    stop_tx: oneshot::Sender<()>,
}

impl Spinner {
    pub fn start(message: &str) -> Self { ... }
    pub fn stop(self) { ... }  // clears the spinner line
}
```

Spawn a Tokio task that updates the spinner character and re-renders every 100ms. When `stop()` is called, send on the channel and the task clears the line with `\r` + spaces and exits.

---

## 5. Changes to existing files

### `src/main.rs`
- Add `--allow-all` as a CLI flag (in addition to the runtime toggle). If passed at launch, start in AutoApprove mode immediately.
- Print the startup banner (Section 3.1) before starting the REPL.
- Pass `permission_mode` into `repl::run()`.

### `src/repl.rs`
- Replace `stdin().read_line()` with the raw-mode character-by-character reader described in Section 1.
- Instantiate `Suggester` from `suggest.rs` and wire up inline completions.
- Detect `--allow-all` typed as input and toggle mode.
- Detect `F2` keypress and toggle mode.
- Wrap all command output in the bordered blocks described in Section 3.3.
- Render the powerline prompt (Section 3.2) instead of the plain prompt.

### `src/agent.rs`
- Start the spinner (Section 4) before calling `llm.plan()`.
- Stop the spinner when the plan arrives.
- Pass `permission_mode` from `context` into `prompt_ui::show_permission_panel()`.
- In AutoApprove mode, run the full plan as a mission loop without pausing between commands.

### `src/prompt_ui.rs`
- Rewrite `show_permission_panel` to the ratatui widget described in Section 3.4.
- Rewrite `show_explanation` to the styled panel in Section 3.5.
- Add `Spinner` struct (Section 4).
- Add `render_status_bar()` for Section 3.6.

### `src/history.rs`
- Add `pub fn all_commands(&self) -> Result<Vec<String>>` — returns all `commands` values from the `turns` table as a flat list of strings, ordered by most recent first. Used by `Suggester` for history-based completion.

### `src/context.rs`
- Add `permission_mode: PermissionMode` field to `Context`.
- Add `PermissionMode` enum.

---

## 6. New files to create

| File | Purpose |
|---|---|
| `src/suggest.rs` | Inline completion engine (Section 1) |
| `src/banner.rs` | Startup ASCII art banner renderer (Section 3.1) |
| `src/spinner.rs` | Async spinner widget (Section 4) |
| `src/blocks.rs` | Command output block renderer (Section 3.3) |
| `src/llm_setup.rs` | Interactive LLM detection and model selection (Section 0A) |
| `src/shell_rc.rs` | Shell rc file read/write for activate and deactivate subcommands (Section 0B) |

---

## 7. New Cargo dependencies to add

Add these to `Cargo.toml` under `[dependencies]`. Do not remove any existing dependencies.

```toml
unicode-width = "0.1"   # for correct string width in terminal rendering
tokio         = { version = "1", features = ["full"] }   # already present, ensure "full"
```

All other required crates (`ratatui`, `crossterm`, `regex`, `dirs`) should already be present from the original build. Verify they are in `Cargo.toml` before adding duplicates.

---

## 8. Updated `Cargo.toml` `[package]` section

Update these fields so the binary installs and identifies correctly:

```toml
[package]
name        = "agentsh"
version     = "0.3.0"
edition     = "2021"
description = "Make any terminal agentic — smart LLM detection, autocomplete, natural language, auto-activation"
license     = "MIT"
repository  = "https://github.com/D9-cell/AGENTSH"
keywords    = ["terminal", "llm", "agent", "ollama", "autocomplete"]
categories  = ["command-line-utilities"]

[[bin]]
name = "agentsh"
path = "src/main.rs"
```

---

## 9. Build order — implement in this exact sequence

Do not skip ahead. Each step depends on the previous.

1. **Fix `install/install.sh` and `install/install.ps1`** — full smart detection + auto-activation injection. Test by running on a clean VM.
2. **`src/shell_rc.rs`** — implement rc file read/write for activate/deactivate. Run `cargo build` after.
3. **`src/llm_setup.rs`** — implement runtime detection and model selection logic used by `select-model` subcommand. Run `cargo build` after.
4. **`src/main.rs`** — add `AGENTSH_ACTIVE` env var export, `deactivate` subcommand, `select-model` subcommand. Run `cargo build` after.
5. **`src/history.rs`** — add `all_commands()` method. Run `cargo test` after.
6. **`src/context.rs`** — add `PermissionMode` enum and field. Run `cargo build` after.
7. **`src/suggest.rs`** — implement `Suggester` with history + builtin + filesystem sources. Write unit tests. Run `cargo test` after.
8. **`src/spinner.rs`** — implement async spinner. Run `cargo build` after.
9. **`src/banner.rs`** — implement banner renderer. Run `cargo build` after.
10. **`src/blocks.rs`** — implement command output block renderer. Run `cargo build` after.
11. **`src/prompt_ui.rs`** — rewrite permission panel, explanation panel, add status bar. Run `cargo build` after.
12. **`src/repl.rs`** — switch to raw-mode reader, wire up suggest, powerline prompt, blocks. Run `cargo build` after.
13. **`src/agent.rs`** — wire up spinner, AutoApprove mission loop. Run `cargo build` after.
14. **`src/main.rs` (final pass)** — add `--allow-all` flag, banner call, pass mode to repl. Run `cargo build --release` after.
15. **End-to-end test** — manually run `./target/release/agentsh` and verify all acceptance criteria below.

---

## 10. Acceptance criteria — all must pass before the build is considered complete

**Install and LLM detection:**
- [ ] Running `install.sh` on a machine with Ollama already present shows the runtime detection menu, does NOT reinstall Ollama, and lets the user pick a model.
- [ ] Running `install.sh` on a machine with no LLM shows the "No runtime detected" menu and correctly installs the chosen runtime.
- [ ] Running `install.sh` on a machine with both Ollama and LM Studio shows both in the selection menu.
- [ ] After install, `~/.agentsh/config.toml` contains the `base_url` and `model` the user selected.
- [ ] The recommendation notice (deepseek-r1, qwen2.5-coder, etc.) is printed at the end of every install.

**Auto-activation:**
- [ ] Opening a new bash or zsh terminal after install starts AgentSH automatically without typing anything.
- [ ] Opening a VS Code integrated terminal does NOT auto-start AgentSH.
- [ ] Running `bash -c "echo hello"` (non-interactive) does NOT trigger AgentSH.
- [ ] Opening a subshell from inside AgentSH (e.g. typing `bash`) does NOT nest AgentSH inside itself.
- [ ] `agentsh deactivate` removes the activation block from the rc file and prints confirmation.
- [ ] After deactivation, opening a new terminal starts normally without AgentSH.
- [ ] `agentsh select-model` shows the model list and updates `~/.agentsh/config.toml` after selection.

**Core functionality:**
- [ ] `agentsh` starts and shows the styled banner.
- [ ] Typing `git st` shows a greyed-out inline suggestion `git status` and Tab accepts it.
- [ ] Typing `ls -la` passes through directly, output appears in a bordered block with exit code and time.
- [ ] Typing `show all files over 10MB` triggers the spinner, then shows the ratatui permission panel.
- [ ] Pressing `n` at the permission panel prints `Cancelled.` and returns to prompt.
- [ ] Pressing `y` executes all commands; output appears in bordered blocks; explanation appears in purple panel.
- [ ] Typing `--allow-all` switches to AutoApprove mode; prompt shows `[AUTO]`.
- [ ] In AutoApprove mode, the permission panel shows a 2-second countdown and auto-approves.
- [ ] CRITICAL-scored commands are never auto-approved even in AutoApprove mode.
- [ ] Pressing `Ctrl-C` during a mission stops execution and resets to PerPlan mode.
- [ ] `cargo clippy -- -D warnings` passes with zero warnings.
- [ ] `cargo test` passes on Linux.
- [ ] The binary on Linux is under 15 MB when built with `--release`.

---

## 11. What NOT to change

- Do not change the LLM system prompt from the original build prompt.
- Do not add streaming of LLM reasoning/thinking.
- Do not add any network calls other than to the configured Ollama base URL and the one-time install downloads.
- Do not add a GUI, web server, or daemon process.
- Do not add `unsafe` Rust blocks.
- Do not replace `ratatui` or `crossterm` with any other TUI library.
- Do not use `rustyline` or `reedline` — the readline must be implemented directly in crossterm raw mode.
- Do not auto-activate in non-interactive shells, IDE terminals, or nested AgentSH sessions.
- Do not modify the user's rc files without printing what was changed.

---

*End of V3 change prompt. Two new features added over V2: smart multi-runtime LLM detection during install (Section 0A) and automatic terminal activation with safe guard conditions (Section 0B). Implement everything in the order defined in Section 9.*
