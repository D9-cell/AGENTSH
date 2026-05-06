$ErrorActionPreference = "Stop"

$Repo = "https://github.com/D9-cell/AGENTSH"
$ConfigDir = Join-Path $env:USERPROFILE ".agentsh"
$ConfigFile = Join-Path $ConfigDir "config.toml"
$InstallDir = Join-Path $env:USERPROFILE ".local\bin"

function Write-Header {
    Write-Host "`n╔══════════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "║         AgentSH — LLM Setup                 ║" -ForegroundColor Cyan
    Write-Host "╚══════════════════════════════════════════════╝`n" -ForegroundColor Cyan
}

function Get-CurrentConfigValue {
    param([string]$Key)

    if (-not (Test-Path $ConfigFile)) {
        return $null
    }

    $match = Select-String -Path $ConfigFile -Pattern "^\s*$Key\s*=\s*\"([^\"]*)\"\s*$" | Select-Object -First 1
    if ($match) {
        return $match.Matches[0].Groups[1].Value
    }

    return $null
}

function Show-ManualConfigHelp {
    Write-Host "`nSet your model runtime manually in $ConfigFile:`n"
    Write-Host "[llm]"
    Write-Host "base_url     = \"http://localhost:11434/v1\""
    Write-Host "model        = \"llama3.1:8b\""
    Write-Host "timeout_secs = 120"
}

function Install-Ollama {
    Write-Host "==> Installing Ollama..." -ForegroundColor Cyan
    winget install Ollama.Ollama --silent --accept-source-agreements --accept-package-agreements
    Start-Sleep -Seconds 8
}

function Install-LMStudio {
    Write-Host "==> Download LM Studio from: https://lmstudio.ai" -ForegroundColor Cyan
    Start-Process "https://lmstudio.ai"
    Read-Host "Press Enter after installing LM Studio and starting its local server" | Out-Null
}

function Install-LocalAI {
    Write-Host "==> Installing LocalAI..." -ForegroundColor Cyan
    if (Get-Command docker -ErrorAction SilentlyContinue) {
        $existing = docker ps -a --format '{{.Names}}' | Where-Object { $_ -eq 'local-ai' }
        if ($existing) {
            docker start local-ai | Out-Null
        }
        else {
            docker run -d -p 8080:8080 --name local-ai localai/localai:latest-cpu | Out-Null
        }
        Write-Host "✓ Started LocalAI on http://localhost:8080" -ForegroundColor Green
        return
    }

    Write-Host "Follow the LocalAI install guide: https://localai.io/installation/" -ForegroundColor Yellow
    Start-Process "https://localai.io/installation/"
    Read-Host "Press Enter after installing LocalAI and starting its API" | Out-Null
}

function Get-RuntimeModels {
    param(
        [string]$Runtime,
        [string]$Url
    )

    try {
        switch ($Runtime) {
            "Ollama" {
                return @(ollama list 2>$null | Select-Object -Skip 1 | ForEach-Object {
                    $parts = ($_ -split '\s+') | Where-Object { $_ }
                    if ($parts.Count -gt 0) { $parts[0] }
                } | Where-Object { $_ })
            }
            "LM Studio" {
                return @(lms ls 2>$null | ForEach-Object {
                    $parts = ($_ -split '\s+') | Where-Object { $_ }
                    if ($parts.Count -gt 0) { $parts[0] }
                } | Select-Object -First 20 | Where-Object { $_ })
            }
            default {
                $response = Invoke-RestMethod -Uri "$Url/models" -Method Get -TimeoutSec 3
                if ($null -ne $response.data) {
                    return @($response.data | ForEach-Object { $_.id } | Where-Object { $_ })
                }
            }
        }
    }
    catch {
    }

    return @()
}

function Ensure-RuntimeModel {
    param(
        [string]$Runtime,
        [string]$Model
    )

    switch ($Runtime) {
        "Ollama" {
            ollama pull $Model
        }
        "LocalAI" {
            if (Get-Command local-ai -ErrorAction SilentlyContinue) {
                try {
                    local-ai models install $Model
                }
                catch {
                    Write-Host "! LocalAI model install failed; keeping model id in config anyway." -ForegroundColor Yellow
                }
            }
        }
    }
}

function Select-Model {
    param(
        [string]$Runtime,
        [string]$Url
    )

    $currentModel = Get-CurrentConfigValue -Key "model"
    $chosenModel = if ($currentModel) { $currentModel } else { "llama3.1:8b" }
    $models = @(Get-RuntimeModels -Runtime $Runtime -Url $Url)
    $index = 1
    $pullIndex = $null
    $manualIndex = $null
    $keepIndex = $null

    Write-Host "`nModels available on $Runtime:`n"
    if ($models.Count -eq 0) {
        Write-Host "  (no models were discovered automatically)" -ForegroundColor Yellow
    }
    else {
        foreach ($model in $models) {
            if ($currentModel -and $model -eq $currentModel) {
                Write-Host "  [$index] $model  (current)"
            }
            else {
                Write-Host "  [$index] $model"
            }
            $index++
        }
    }

    if ($Runtime -eq "Ollama") {
        Write-Host "  [$index] Pull a new model..."
        $pullIndex = $index
        $index++
    }

    Write-Host "  [$index] Enter a model id manually"
    $manualIndex = $index
    $index++

    if ($currentModel) {
        Write-Host "  [$index] Keep current config"
        $keepIndex = $index
        $index++
    }

    Write-Host "`nRecommended for AgentSH: llama3.1:8b or qwen2.5-coder:14b"
    $choice = Read-Host "Enter your choice [1]"
    if (-not $choice) { $choice = "1" }

    $choiceValue = 0
    [void][int]::TryParse($choice, [ref]$choiceValue)

    if ($choiceValue -ge 1 -and $choiceValue -le $models.Count) {
        $chosenModel = $models[$choiceValue - 1]
    }
    elseif ($pullIndex -and $choiceValue -eq $pullIndex) {
        $chosenModel = Read-Host "Enter model name to pull (e.g. deepseek-r1:8b)"
        if (-not $chosenModel) { $chosenModel = "llama3.1:8b" }
        Ensure-RuntimeModel -Runtime $Runtime -Model $chosenModel
    }
    elseif ($choiceValue -eq $manualIndex) {
        $chosenModel = Read-Host "Enter model id"
        if (-not $chosenModel) {
            $chosenModel = if ($currentModel) { $currentModel } else { "llama3.1:8b" }
        }
    }
    elseif ($keepIndex -and $choiceValue -eq $keepIndex) {
        $chosenModel = $currentModel
    }
    elseif ($models.Count -gt 0) {
        $chosenModel = $models[0]
    }

    Write-Host "`n✓ Selected: $chosenModel via $Runtime`n" -ForegroundColor Green
    Write-Host "💡 Recommended models for best results with AgentSH:"
    Write-Host "   • deepseek-r1:8b     — best reasoning, great for complex tasks"
    Write-Host "   • qwen2.5-coder:14b  — best for coding and file editing"
    Write-Host "   • mistral-nemo       — fastest tool-calling"
    Write-Host "   Pull any of these later with: ollama pull <model-name>"
    return $chosenModel
}

function Write-Config {
    param(
        [string]$BaseUrl,
        [string]$Model
    )

    New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null
    @"
[llm]
base_url     = "$BaseUrl"
model        = "$Model"
timeout_secs = 120

[safety]
require_confirm   = true
auto_approve_safe = false

[agent]
max_commands_per_turn = 8
context_lines         = 40
"@ | Set-Content -Path $ConfigFile

    Write-Host "✓ Config written to $ConfigFile" -ForegroundColor Green
}

function Install-Binary {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Force -Path $tmp | Out-Null
        git clone --depth 1 $Repo (Join-Path $tmp "agentsh")
        Push-Location (Join-Path $tmp "agentsh")
        try {
            cargo build --release
            Copy-Item "target\release\agentsh.exe" (Join-Path $InstallDir "agentsh.exe") -Force
        }
        finally {
            Pop-Location
            Remove-Item $tmp -Recurse -Force
        }
        return
    }

    $url = "$Repo/releases/latest/download/agentsh-windows-x86_64.exe"
    Invoke-WebRequest $url -OutFile (Join-Path $InstallDir "agentsh.exe")
}

function Ensure-PathContainsInstallDir {
    $currentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if (-not $currentPath) {
        [Environment]::SetEnvironmentVariable("PATH", $InstallDir, "User")
        return
    }

    if ($currentPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("PATH", "$currentPath;$InstallDir", "User")
    }
}

function Configure-AutoActivation {
    $profilePath = $PROFILE.CurrentUserCurrentHost
    $profileDir = Split-Path -Parent $profilePath
    if ($profileDir -and -not (Test-Path $profileDir)) {
        New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
    }
    if (-not (Test-Path $profilePath)) {
        New-Item -ItemType File -Force -Path $profilePath | Out-Null
    }

    $activationBlock = @'
# AgentSH auto-activation
if ([Environment]::GetEnvironmentVariable("AGENTSH_ACTIVE") -ne "1" -and
    $env:TERM_PROGRAM -ne "vscode" -and
    $env:TERM_PROGRAM -ne "jetbrains" -and
    (Get-Command agentsh -ErrorAction SilentlyContinue)) {
    agentsh
}
'@

    if (Select-String -Path $profilePath -Pattern "AgentSH auto-activation" -Quiet) {
        Write-Host "! Auto-activation already configured in $profilePath" -ForegroundColor Yellow
        return
    }

    Add-Content -Path $profilePath -Value "`n$activationBlock"
    Write-Host "✓ Added AgentSH auto-activation to $profilePath" -ForegroundColor Green
}

Write-Header
Write-Host "Scanning for local LLM runtimes..."

$runtimes = New-Object System.Collections.Generic.List[string]
$runtimeUrls = @{}

$checks = @(
    @{ Name = "Ollama"; Cmd = "ollama"; Url = "http://localhost:11434/v1" },
    @{ Name = "LM Studio"; Cmd = "lms"; Url = "http://localhost:1234/v1" },
    @{ Name = "LocalAI"; Cmd = "local-ai"; Url = "http://localhost:8080/v1" },
    @{ Name = "Jan"; Cmd = "jan"; Url = "http://localhost:1337/v1" }
)

foreach ($check in $checks) {
    if (Get-Command $check.Cmd -ErrorAction SilentlyContinue) {
        $runtimes.Add($check.Name)
        $runtimeUrls[$check.Name] = $check.Url
        Write-Host "  ✓ $($check.Name) (detected)" -ForegroundColor Green
    }
}

if ((Get-Command "llama-server" -ErrorAction SilentlyContinue) -or (Get-Command "llama.cpp" -ErrorAction SilentlyContinue)) {
    $runtimes.Add("llama.cpp")
    $runtimeUrls["llama.cpp"] = "http://localhost:8080/v1"
    Write-Host "  ✓ llama.cpp (detected)" -ForegroundColor Green
}

$chosenRuntime = $null
$chosenUrl = $null
$chosenModel = $null

if ($runtimes.Count -eq 0) {
    Write-Host "`nNo local LLM runtime detected on this machine.`n" -ForegroundColor Yellow
    Write-Host "AgentSH requires a local LLM to function. Choose one to install:`n"
    Write-Host "  [1] Ollama          (recommended — easiest setup, widest model support)"
    Write-Host "  [2] LM Studio       (GUI app with model manager — good for beginners)"
    Write-Host "  [3] LocalAI         (self-hosted, OpenAI-compatible server)"
    Write-Host "  [4] Skip — I'll install manually and configure later`n"
    $choice = Read-Host "Enter your choice [1]"
    if (-not $choice) { $choice = "1" }

    switch ($choice) {
        "1" {
            Install-Ollama
            $chosenRuntime = "Ollama"
            $chosenUrl = "http://localhost:11434/v1"
        }
        "2" {
            Install-LMStudio
            $chosenRuntime = "LM Studio"
            $chosenUrl = "http://localhost:1234/v1"
        }
        "3" {
            Install-LocalAI
            $chosenRuntime = "LocalAI"
            $chosenUrl = "http://localhost:8080/v1"
        }
        default {
            $chosenRuntime = "manual"
        }
    }
}
else {
    Write-Host "`nFound the following local LLM runtimes on your system:`n"
    for ($i = 0; $i -lt $runtimes.Count; $i++) {
        Write-Host "  [$($i + 1)] $($runtimes[$i])          (detected)"
    }
    Write-Host "  [$($runtimes.Count + 1)] Install Ollama  (recommended — broadest model support)"
    Write-Host "  [$($runtimes.Count + 2)] Install nothing — I'll configure manually later`n"

    $choice = Read-Host "Enter your choice [1]"
    if (-not $choice) { $choice = "1" }

    $index = 0
    if ([int]::TryParse($choice, [ref]$index) -and $index -ge 1 -and $index -le $runtimes.Count) {
        $chosenRuntime = $runtimes[$index - 1]
        $chosenUrl = $runtimeUrls[$chosenRuntime]
    }
    elseif ($choice -eq [string]($runtimes.Count + 1)) {
        Install-Ollama
        $chosenRuntime = "Ollama"
        $chosenUrl = "http://localhost:11434/v1"
    }
    else {
        $chosenRuntime = "manual"
    }
}

if ($chosenRuntime -eq "manual") {
    Show-ManualConfigHelp
}
else {
    $chosenModel = Select-Model -Runtime $chosenRuntime -Url $chosenUrl
    Write-Config -BaseUrl $chosenUrl -Model $chosenModel
}

Write-Host "`n💡 Model recommendations for AgentSH:"
Write-Host "   Best overall:     deepseek-r1:8b      (ollama pull deepseek-r1:8b)"
Write-Host "   Best for coding:  qwen2.5-coder:14b   (ollama pull qwen2.5-coder:14b)"
Write-Host "   Fastest:          llama3.1:8b         (ollama pull llama3.1:8b)"
Write-Host "   Low RAM (4GB):    phi3.5-mini         (ollama pull phi3.5-mini)"

Write-Host "`n==> Installing agentsh binary..." -ForegroundColor Cyan
Install-Binary
Ensure-PathContainsInstallDir

Write-Host "`n==> Configuring auto-activation..." -ForegroundColor Cyan
Configure-AutoActivation

Write-Host "`n✓ AgentSH installed and configured!" -ForegroundColor Green
Write-Host "  Open a new terminal and AgentSH will start automatically."
Write-Host "  Or run now: agentsh"