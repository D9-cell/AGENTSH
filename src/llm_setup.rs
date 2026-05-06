use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use tokio::process::Command;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Ollama,
    LmStudio,
    LocalAi,
    LlamaCpp,
    Jan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInfo {
    pub kind: RuntimeKind,
    pub name: &'static str,
    pub base_url: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmSelection {
    pub runtime: RuntimeInfo,
    pub model: String,
}

struct RuntimeSpec {
    kind: RuntimeKind,
    name: &'static str,
    base_url: &'static str,
    commands: &'static [&'static str],
}

const RUNTIME_SPECS: &[RuntimeSpec] = &[
    RuntimeSpec {
        kind: RuntimeKind::Ollama,
        name: "Ollama",
        base_url: "http://localhost:11434/v1",
        commands: &["ollama"],
    },
    RuntimeSpec {
        kind: RuntimeKind::LmStudio,
        name: "LM Studio",
        base_url: "http://localhost:1234/v1",
        commands: &["lms"],
    },
    RuntimeSpec {
        kind: RuntimeKind::LocalAi,
        name: "LocalAI",
        base_url: "http://localhost:8080/v1",
        commands: &["local-ai"],
    },
    RuntimeSpec {
        kind: RuntimeKind::LlamaCpp,
        name: "llama.cpp",
        base_url: "http://localhost:8080/v1",
        commands: &["llama-server", "llama.cpp"],
    },
    RuntimeSpec {
        kind: RuntimeKind::Jan,
        name: "Jan",
        base_url: "http://localhost:1337/v1",
        commands: &["jan"],
    },
];

pub fn detect_runtimes() -> Vec<RuntimeInfo> {
    RUNTIME_SPECS
        .iter()
        .filter(|spec| spec.commands.iter().any(|command| binary_in_path(command)))
        .map(|spec| RuntimeInfo {
            kind: spec.kind,
            name: spec.name,
            base_url: spec.base_url,
        })
        .collect()
}

pub async fn select_model_interactively(config: &Config) -> Result<LlmSelection> {
    let runtimes = detect_runtimes();
    if runtimes.is_empty() {
        return Err(anyhow!(
            "no supported local LLM runtime detected in PATH; configure ~/.agentsh/config.toml manually"
        ));
    }

    let runtime = choose_runtime(config, &runtimes)?;
    let models = list_models(&runtime).await.unwrap_or_default();
    let model = choose_model(&runtime, &config.llm.model, &models).await?;

    Ok(LlmSelection { runtime, model })
}

pub fn write_selection(selection: &LlmSelection) -> Result<()> {
    let path = Config::config_path()?;
    let mut config = Config::load()?;
    config.llm.base_url = selection.runtime.base_url.to_string();
    config.llm.model = selection.model.clone();

    let contents = toml::to_string_pretty(&config)?;
    fs::write(&path, contents)
        .with_context(|| format!("failed to write config file at {}", path.display()))?;
    Ok(())
}

fn choose_runtime(config: &Config, runtimes: &[RuntimeInfo]) -> Result<RuntimeInfo> {
    let default_choice = runtimes
        .iter()
        .position(|runtime| runtime.base_url == config.llm.base_url)
        .map(|index| index + 1)
        .unwrap_or(1);

    if runtimes.len() == 1 {
        return Ok(runtimes[0].clone());
    }

    println!("Found the following local LLM runtimes on your system:\n");
    for (index, runtime) in runtimes.iter().enumerate() {
        if runtime.base_url == config.llm.base_url {
            println!("  [{}] {}  (current)", index + 1, runtime.name);
        } else {
            println!("  [{}] {}", index + 1, runtime.name);
        }
    }

    let choice = read_choice(&format!("Enter your choice [{default_choice}]: "), default_choice)?;
    Ok(runtimes
        .get(choice.saturating_sub(1))
        .cloned()
        .unwrap_or_else(|| runtimes[default_choice - 1].clone()))
}

async fn choose_model(runtime: &RuntimeInfo, current_model: &str, models: &[String]) -> Result<String> {
    let current_index = models.iter().position(|model| model == current_model);
    let mut next_index = 1usize;

    println!("\nModels available on {}:\n", runtime.name);
    if models.is_empty() {
        println!("  (no models were discovered automatically)");
    } else {
        for model in models {
            if Some(next_index - 1) == current_index {
                println!("  [{next_index}] {model}  (current)");
            } else {
                println!("  [{next_index}] {model}");
            }
            next_index += 1;
        }
    }

    let pull_index = if matches!(runtime.kind, RuntimeKind::Ollama) {
        let index = next_index;
        println!("  [{index}] Pull a new model...");
        next_index += 1;
        Some(index)
    } else {
        None
    };

    let manual_index = next_index;
    println!("  [{manual_index}] Enter a model id manually");
    next_index += 1;

    let keep_index = if current_index.is_none() && !current_model.trim().is_empty() {
        let index = next_index;
        println!("  [{index}] Keep current config");
        Some(index)
    } else {
        None
    };

    println!(
        "\nRecommended: deepseek-r1:8b (best reasoning) · qwen2.5-coder:14b (best coding)"
    );

    let default_choice = current_index.map(|index| index + 1).unwrap_or(1);
    let choice = read_choice(&format!("Enter your choice [{default_choice}]: "), default_choice)?;

    if (1..=models.len()).contains(&choice) {
        return Ok(models[choice - 1].clone());
    }

    if Some(choice) == pull_index {
        let model = read_text_prompt("Enter model name to pull (e.g. deepseek-r1:8b): ")?;
        let model = if model.trim().is_empty() {
            "llama3.1:8b".to_string()
        } else {
            model
        };
        pull_model(runtime, &model).await?;
        return Ok(model);
    }

    if choice == manual_index {
        let model = read_text_prompt("Enter model id: ")?;
        if model.trim().is_empty() {
            return Ok(fallback_model(current_model, models));
        }
        return Ok(model);
    }

    if Some(choice) == keep_index {
        return Ok(current_model.to_string());
    }

    Ok(fallback_model(current_model, models))
}

async fn list_models(runtime: &RuntimeInfo) -> Result<Vec<String>> {
    match runtime.kind {
        RuntimeKind::Ollama => list_models_from_command("ollama", &["list"], true).await,
        RuntimeKind::LmStudio => {
            let mut models = list_models_from_command("lms", &["ls"], false).await?;
            models.truncate(20);
            Ok(models)
        }
        RuntimeKind::LocalAi | RuntimeKind::LlamaCpp | RuntimeKind::Jan => {
            list_models_from_api(runtime.base_url).await
        }
    }
}

async fn list_models_from_command(binary: &str, args: &[&str], skip_first_line: bool) -> Result<Vec<String>> {
    let output = Command::new(binary)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .with_context(|| format!("failed to run {binary}"))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = stdout
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if skip_first_line && index == 0 {
                return None;
            }
            line.split_whitespace().next().map(str::to_string)
        })
        .collect();

    Ok(models)
}

async fn list_models_from_api(base_url: &str) -> Result<Vec<String>> {
    let response = reqwest::get(format!("{base_url}/models"))
        .await
        .with_context(|| format!("failed to query {base_url}/models"))?;

    if !response.status().is_success() {
        return Ok(Vec::new());
    }

    let payload = response.json::<serde_json::Value>().await?;
    let models = payload
        .get("data")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(models)
}

async fn pull_model(runtime: &RuntimeInfo, model: &str) -> Result<()> {
    if !matches!(runtime.kind, RuntimeKind::Ollama) {
        return Ok(());
    }

    let status = Command::new("ollama")
        .arg("pull")
        .arg(model)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .context("failed to run ollama pull")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("ollama pull failed for model {model}"))
    }
}

fn fallback_model(current_model: &str, models: &[String]) -> String {
    if !current_model.trim().is_empty() {
        current_model.to_string()
    } else {
        models
            .first()
            .cloned()
            .unwrap_or_else(|| "llama3.1:8b".to_string())
    }
}

fn read_choice(prompt: &str, default: usize) -> Result<usize> {
    let value = read_text_prompt(prompt)?;
    if value.trim().is_empty() {
        return Ok(default);
    }

    Ok(value.trim().parse::<usize>().unwrap_or(default))
}

fn read_text_prompt(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush().context("failed to flush prompt")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read terminal input")?;
    Ok(input.trim().to_string())
}

fn binary_in_path(binary: &str) -> bool {
    path_candidates(binary).into_iter().any(|candidate| candidate.is_file())
}

fn path_candidates(binary: &str) -> Vec<PathBuf> {
    let path = env::var_os("PATH").unwrap_or_default();
    let suffixes = executable_suffixes();
    let mut candidates = Vec::new();

    for directory in env::split_paths(&path) {
        for suffix in &suffixes {
            let mut file_name = OsString::from(binary);
            file_name.push(suffix);
            candidates.push(directory.join(Path::new(&file_name)));
        }
    }

    candidates
}

fn executable_suffixes() -> Vec<OsString> {
    if !cfg!(windows) {
        return vec![OsString::new()];
    }

    env::var_os("PATHEXT")
        .map(|value| {
            env::split_paths(&value)
                .map(|suffix| suffix.as_os_str().to_os_string())
                .collect::<Vec<_>>()
        })
        .filter(|suffixes| !suffixes.is_empty())
        .unwrap_or_else(|| vec![OsString::from(".exe"), OsString::from(".cmd"), OsString::from(".bat")])
}
