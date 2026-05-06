#!/usr/bin/env bash
set -euo pipefail

REPO="https://github.com/D9-cell/AGENTSH"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="$HOME/.agentsh"
CONFIG_FILE="$CONFIG_DIR/config.toml"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

header() {
  printf "\n${CYAN}${BOLD}╔══════════════════════════════════════════════╗\n"
  printf "║         AgentSH — LLM Setup                 ║\n"
  printf "╚══════════════════════════════════════════════╝${RESET}\n\n"
}

info() {
  printf "%b==>%b %s\n" "$CYAN" "$RESET" "$1"
}

warn() {
  printf "%b!%b %s\n" "$YELLOW" "$RESET" "$1"
}

success() {
  printf "%b✓%b %s\n" "$GREEN" "$RESET" "$1"
}

current_config_value() {
  local key="$1"

  [[ -f "$CONFIG_FILE" ]] || return 0
  sed -nE "s/^[[:space:]]*${key}[[:space:]]*=[[:space:]]*\"([^\"]*)\"[[:space:]]*$/\1/p" "$CONFIG_FILE" | head -n 1
}

print_manual_config_help() {
  printf "\nSet your model runtime manually in %s:\n\n" "$CONFIG_FILE"
  cat <<EOF
[llm]
base_url     = "http://localhost:11434/v1"
model        = "llama3.1:8b"
timeout_secs = 120
EOF
}

install_ollama() {
  info "Installing Ollama..."
  curl -fsSL https://ollama.com/install.sh | sh
}

install_lm_studio() {
  info "LM Studio is installed manually from https://lmstudio.ai"
  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "https://lmstudio.ai" >/dev/null 2>&1 || true
  fi
  printf "Press Enter after installing LM Studio and starting its local server... "
  read -r
}

install_localai() {
  info "Installing LocalAI..."
  if command -v docker >/dev/null 2>&1; then
    if docker ps -a --format '{{.Names}}' | grep -qx 'local-ai'; then
      docker start local-ai >/dev/null
    else
      docker run -d -p 8080:8080 --name local-ai localai/localai:latest-cpu >/dev/null
    fi
    success "Started LocalAI on http://localhost:8080"
    return 0
  fi

  warn "Docker was not found. Follow the LocalAI install guide: https://localai.io/installation/linux/"
  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "https://localai.io/installation/linux/" >/dev/null 2>&1 || true
  fi
  printf "Press Enter after installing LocalAI and starting its API... "
  read -r
}

list_runtime_models() {
  local runtime="$1"
  local url="$2"

  case "$runtime" in
    "Ollama")
      ollama list 2>/dev/null | awk 'NR > 1 && NF { print $1 }'
      ;;
    "LM Studio")
      lms ls 2>/dev/null | awk 'NF { print $1 }' | head -20
      ;;
    *)
      curl -fsS "$url/models" 2>/dev/null \
        | grep -oE '"id"[[:space:]]*:[[:space:]]*"[^"]+"' \
        | sed -E 's/.*"([^"]+)"$/\1/'
      ;;
  esac
}

ensure_runtime_model() {
  local runtime="$1"
  local model="$2"

  case "$runtime" in
    "Ollama")
      ollama pull "$model"
      ;;
    "LocalAI")
      if command -v local-ai >/dev/null 2>&1; then
        local-ai models install "$model" || warn "LocalAI model installation failed; keeping model id in config anyway."
      fi
      ;;
  esac
}

choose_model() {
  local runtime="$1"
  local url="$2"
  local current_model
  local choice
  local idx
  local pull_index=0
  local manual_index=0
  local keep_index=0
  local -a models=()

  current_model="$(current_config_value model)"
  CHOSEN_MODEL="${current_model:-llama3.1:8b}"

  while IFS= read -r model; do
    [[ -n "$model" ]] || continue
    models+=("$model")
  done < <(list_runtime_models "$runtime" "$url" || true)

  printf "\nModels available on %s:\n\n" "$runtime"
  idx=1
  if [[ ${#models[@]} -eq 0 ]]; then
    warn "No models were discovered automatically."
  else
    for model in "${models[@]}"; do
      if [[ -n "$current_model" && "$model" == "$current_model" ]]; then
        printf "  [%d] %s  (current)\n" "$idx" "$model"
      else
        printf "  [%d] %s\n" "$idx" "$model"
      fi
      ((idx++))
    done
  fi

  if [[ "$runtime" == "Ollama" ]]; then
    printf "  [%d] Pull a new model...\n" "$idx"
    pull_index=$idx
    ((idx++))
  fi

  printf "  [%d] Enter a model id manually\n" "$idx"
  manual_index=$idx
  ((idx++))

  if [[ -n "$current_model" ]]; then
    printf "  [%d] Keep current config\n" "$idx"
    keep_index=$idx
    ((idx++))
  fi

  printf "\nRecommended for AgentSH: llama3.1:8b or qwen2.5-coder:14b\n"
  printf "Enter your choice [1]: "
  read -r choice
  choice=${choice:-1}

  if [[ "$choice" =~ ^[0-9]+$ ]] && (( choice >= 1 && choice <= ${#models[@]} )); then
    CHOSEN_MODEL="${models[$((choice - 1))]}"
  elif (( pull_index > 0 )) && [[ "$choice" == "$pull_index" ]]; then
    printf "Enter model name to pull (e.g. deepseek-r1:8b): "
    read -r CHOSEN_MODEL
    if [[ -z "$CHOSEN_MODEL" ]]; then
      CHOSEN_MODEL="llama3.1:8b"
    fi
    ensure_runtime_model "$runtime" "$CHOSEN_MODEL"
  elif [[ "$choice" == "$manual_index" ]]; then
    printf "Enter model id: "
    read -r CHOSEN_MODEL
    if [[ -z "$CHOSEN_MODEL" ]]; then
      CHOSEN_MODEL="${current_model:-llama3.1:8b}"
    fi
  elif (( keep_index > 0 )) && [[ "$choice" == "$keep_index" ]]; then
    CHOSEN_MODEL="$current_model"
  elif [[ ${#models[@]} -gt 0 ]]; then
    CHOSEN_MODEL="${models[0]}"
  fi

  printf "\n%b✓%b Selected: %s via %s\n\n" "$GREEN" "$RESET" "$CHOSEN_MODEL" "$runtime"
  printf "💡 Recommended models for best results with AgentSH:\n"
  printf "   • deepseek-r1:8b     — best reasoning, great for complex tasks\n"
  printf "   • qwen2.5-coder:14b  — best for coding and file editing\n"
  printf "   • mistral-nemo       — fastest tool-calling\n"
  printf "   Pull any of these later with: ollama pull <model-name>\n"
}

write_config() {
  mkdir -p "$CONFIG_DIR"
  cat > "$CONFIG_FILE" <<EOF
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
EOF
  success "Config written to $CONFIG_FILE"
}

install_binary_file() {
  local source_path="$1"
  local target_path="$INSTALL_DIR/agentsh"

  if [[ ! -d "$INSTALL_DIR" ]]; then
    if [[ -w "$(dirname "$INSTALL_DIR")" ]]; then
      mkdir -p "$INSTALL_DIR"
    else
      sudo mkdir -p "$INSTALL_DIR"
    fi
  fi

  if [[ -w "$INSTALL_DIR" ]]; then
    install -m 0755 "$source_path" "$target_path"
  else
    sudo install -m 0755 "$source_path" "$target_path"
  fi
}

install_binary() {
  local os
  local arch
  local url
  local tmp_dir

  info "Installing agentsh binary..."
  if command -v cargo >/dev/null 2>&1; then
    tmp_dir="$(mktemp -d)"
    git clone --depth 1 "$REPO" "$tmp_dir/agentsh"
    (
      cd "$tmp_dir/agentsh"
      cargo build --release
    )
    install_binary_file "$tmp_dir/agentsh/target/release/agentsh"
    rm -rf "$tmp_dir"
    return 0
  fi

  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$arch" in
    x86_64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)
      printf "Unsupported architecture: %s\n" "$arch" >&2
      exit 1
      ;;
  esac

  url="$REPO/releases/latest/download/agentsh-${os}-${arch}"
  tmp_dir="$(mktemp -d)"
  curl -fsSL "$url" -o "$tmp_dir/agentsh"
  chmod +x "$tmp_dir/agentsh"
  install_binary_file "$tmp_dir/agentsh"
  rm -rf "$tmp_dir"
}

append_block_if_missing() {
  local rc_file="$1"
  local block="$2"

  mkdir -p "$(dirname "$rc_file")"
  touch "$rc_file"

  if grep -q "AgentSH auto-activation" "$rc_file"; then
    warn "Auto-activation already configured in $rc_file"
    return 0
  fi

  printf "\n%s\n" "$block" >> "$rc_file"
  success "Added AgentSH auto-activation to $rc_file"
}

configure_auto_activation() {
  local shell_name
  local bash_block
  local fish_block

  bash_block=$(cat <<'EOF'
# AgentSH auto-activation
if [ -t 1 ] && \
   [ -z "$AGENTSH_ACTIVE" ] && \
   [ "$TERM_PROGRAM" != "vscode" ] && \
   [ "$TERM_PROGRAM" != "jetbrains" ] && \
   command -v agentsh > /dev/null 2>&1; then
  exec agentsh
fi
EOF
)

  fish_block=$(cat <<'EOF'
# AgentSH auto-activation
if status is-interactive
    and test -z "$AGENTSH_ACTIVE"
    and test "$TERM_PROGRAM" != "vscode"
    and test "$TERM_PROGRAM" != "jetbrains"
    and command -v agentsh > /dev/null 2>&1
    exec agentsh
end
EOF
)

  info "Configuring auto-activation..."
  shell_name="$(basename "${SHELL:-}")"
  case "$shell_name" in
    bash)
      append_block_if_missing "$HOME/.bashrc" "$bash_block"
      append_block_if_missing "$HOME/.bash_profile" "$bash_block"
      ;;
    zsh)
      append_block_if_missing "$HOME/.zshrc" "$bash_block"
      ;;
    fish)
      append_block_if_missing "$HOME/.config/fish/conf.d/agentsh.fish" "$fish_block"
      ;;
    *)
      warn "Unknown shell '${shell_name:-unknown}'. Add this block manually:"
      printf "%s\n" "$bash_block"
      ;;
  esac
}

declare -a DETECTED=()
declare -A RUNTIME_URL=()

detect_runtime() {
  local name="$1"
  local cmd="$2"
  local url="$3"

  if command -v "$cmd" >/dev/null 2>&1; then
    DETECTED+=("$name")
    RUNTIME_URL["$name"]="$url"
    printf "  %b✓%b %s (detected)\n" "$GREEN" "$RESET" "$name"
  fi
}

header
printf "Scanning for local LLM runtimes...\n"
detect_runtime "Ollama" "ollama" "http://localhost:11434/v1"
detect_runtime "LM Studio" "lms" "http://localhost:1234/v1"
detect_runtime "LocalAI" "local-ai" "http://localhost:8080/v1"
if command -v llama-server >/dev/null 2>&1 || command -v llama.cpp >/dev/null 2>&1; then
  DETECTED+=("llama.cpp")
  RUNTIME_URL["llama.cpp"]="http://localhost:8080/v1"
  printf "  %b✓%b llama.cpp (detected)\n" "$GREEN" "$RESET"
fi
detect_runtime "Jan" "jan" "http://localhost:1337/v1"

CHOSEN_RUNTIME=""
CHOSEN_URL=""
CHOSEN_MODEL=""

if [[ ${#DETECTED[@]} -eq 0 ]]; then
  printf "\n${YELLOW}No local LLM runtime detected on this machine.${RESET}\n\n"
  printf "AgentSH requires a local LLM to function. Choose one to install:\n\n"
  printf "  [1] Ollama          (recommended — easiest setup, widest model support)\n"
  printf "  [2] LM Studio       (GUI app with model manager — good for beginners)\n"
  printf "  [3] LocalAI         (self-hosted, OpenAI-compatible server)\n"
  printf "  [4] Skip — I'll install manually and configure later\n\n"
  printf "Enter your choice [1]: "
  read -r choice
  choice=${choice:-1}

  case "$choice" in
    1)
      install_ollama
      CHOSEN_RUNTIME="Ollama"
      CHOSEN_URL="http://localhost:11434/v1"
      ;;
    2)
      install_lm_studio
      CHOSEN_RUNTIME="LM Studio"
      CHOSEN_URL="http://localhost:1234/v1"
      ;;
    3)
      install_localai
      CHOSEN_RUNTIME="LocalAI"
      CHOSEN_URL="http://localhost:8080/v1"
      ;;
    4)
      CHOSEN_RUNTIME="manual"
      ;;
    *)
      CHOSEN_RUNTIME="Ollama"
      CHOSEN_URL="http://localhost:11434/v1"
      install_ollama
      ;;
  esac
else
  printf "\nFound the following local LLM runtimes on your system:\n\n"
  for i in "${!DETECTED[@]}"; do
    printf "  [%d] %s          (detected)\n" "$((i + 1))" "${DETECTED[$i]}"
  done
  printf "  [%d] Install Ollama  (recommended — broadest model support)\n" "$(( ${#DETECTED[@]} + 1 ))"
  printf "  [%d] Install nothing — I'll configure manually later\n\n" "$(( ${#DETECTED[@]} + 2 ))"
  printf "Enter your choice [1]: "
  read -r choice
  choice=${choice:-1}

  if [[ "$choice" =~ ^[0-9]+$ ]] && (( choice >= 1 && choice <= ${#DETECTED[@]} )); then
    CHOSEN_RUNTIME="${DETECTED[$((choice - 1))]}"
    CHOSEN_URL="${RUNTIME_URL[$CHOSEN_RUNTIME]}"
  elif [[ "$choice" == "$(( ${#DETECTED[@]} + 1 ))" ]]; then
    install_ollama
    CHOSEN_RUNTIME="Ollama"
    CHOSEN_URL="http://localhost:11434/v1"
  else
    CHOSEN_RUNTIME="manual"
  fi
fi

if [[ "$CHOSEN_RUNTIME" == "manual" ]]; then
  print_manual_config_help
else
  choose_model "$CHOSEN_RUNTIME" "$CHOSEN_URL"
  write_config
fi

printf "\n💡 Model recommendations for AgentSH:\n"
printf "   Best overall:     deepseek-r1:8b      (ollama pull deepseek-r1:8b)\n"
printf "   Best for coding:  qwen2.5-coder:14b   (ollama pull qwen2.5-coder:14b)\n"
printf "   Fastest:          llama3.1:8b         (ollama pull llama3.1:8b)\n"
printf "   Low RAM (4GB):    phi3.5-mini         (ollama pull phi3.5-mini)\n"

install_binary
configure_auto_activation

printf "\n${GREEN}${BOLD}✓ AgentSH installed and configured!${RESET}\n"
printf "  Open a new terminal and AgentSH will start automatically.\n"
printf "  Or run now: agentsh\n\n"