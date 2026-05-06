$ErrorActionPreference = "Stop"

Write-Host "==> Checking for Ollama..."
if (-not (Get-Command ollama -ErrorAction SilentlyContinue)) {
    Write-Host "==> Installing Ollama..."
    winget install Ollama.Ollama --silent
    Start-Sleep -Seconds 5
}

$model = if ($env:AGENTSH_MODEL) { $env:AGENTSH_MODEL } else { "llama3.1:8b" }
Write-Host "==> Pulling model: $model"
ollama pull $model

Write-Host "==> Installing agentsh..."
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    cargo install agentsh
} else {
    $url = "https://github.com/YOUR_USERNAME/agentsh/releases/latest/download/agentsh-windows-x86_64.exe"
    Invoke-WebRequest $url -OutFile "$env:USERPROFILE\.cargo\bin\agentsh.exe"
}

Write-Host ""
Write-Host "Done! Run: agentsh"