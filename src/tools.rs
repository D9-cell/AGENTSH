use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::safety::{self, RiskLevel};

const CWD_SENTINEL: &str = "__AGENTSH_CWD__=";

#[derive(Debug, Clone, Serialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolDefinition,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct PlannedCommand {
    pub tool_name: String,
    pub args: Value,
    pub risk: RiskLevel,
    pub display_text: String,
    pub preview: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolExecution {
    pub output: String,
    pub exit_code: Option<i32>,
    pub updated_cwd: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct BashExecArgs {
    command: String,
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FileReadArgs {
    path: String,
    max_lines: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct FileWriteArgs {
    path: String,
    content: String,
}

pub fn all_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            kind: "function".to_string(),
            function: ToolDefinition {
                name: "bash_exec".to_string(),
                description: "Run a shell command in the current directory or an explicit working directory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute."
                        },
                        "cwd": {
                            "type": "string",
                            "description": "Optional working directory for the command."
                        }
                    },
                    "required": ["command"],
                    "additionalProperties": false
                }),
            },
        },
        ToolSchema {
            kind: "function".to_string(),
            function: ToolDefinition {
                name: "file_read".to_string(),
                description: "Read a text file from disk.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read."
                        },
                        "max_lines": {
                            "type": "integer",
                            "description": "Optional maximum number of lines to return. Defaults to 200."
                        }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }),
            },
        },
        ToolSchema {
            kind: "function".to_string(),
            function: ToolDefinition {
                name: "file_write".to_string(),
                description: "Write content to a file, creating or overwriting it.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write."
                        },
                        "content": {
                            "type": "string",
                            "description": "The full file contents to write."
                        }
                    },
                    "required": ["path", "content"],
                    "additionalProperties": false
                }),
            },
        },
        ToolSchema {
            kind: "function".to_string(),
            function: ToolDefinition {
                name: "git_status".to_string(),
                description: "Run git status --short in the current directory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": [],
                    "additionalProperties": false
                }),
            },
        },
    ]
}

pub fn describe_tool_call(name: &str, args: &Value) -> Result<String> {
    match name {
        "bash_exec" => {
            let parsed: BashExecArgs = serde_json::from_value(args.clone())?;
            Ok(parsed.command)
        }
        "file_read" => {
            let parsed: FileReadArgs = serde_json::from_value(args.clone())?;
            Ok(format!("read {}", parsed.path))
        }
        "file_write" => {
            let parsed: FileWriteArgs = serde_json::from_value(args.clone())?;
            Ok(format!("write {}", parsed.path))
        }
        "git_status" => Ok("git status --short".to_string()),
        other => Err(anyhow!("unknown tool: {other}")),
    }
}

pub fn risk_for_plan(name: &str, args: &Value) -> Result<RiskLevel> {
    if name == "bash_exec" {
        let parsed: BashExecArgs = serde_json::from_value(args.clone())?;
        return Ok(safety::score(&parsed.command));
    }

    Ok(RiskLevel::Safe)
}

pub async fn preview_tool_call(name: &str, args: &Value, current_cwd: &Path) -> Result<Option<String>> {
    if name != "file_write" {
        return Ok(None);
    }

    let parsed: FileWriteArgs = serde_json::from_value(args.clone())?;
    let path = resolve_path(current_cwd, &parsed.path);
    if !path.exists() {
        return Ok(None);
    }

    let existing = match fs::read_to_string(&path).await {
        Ok(content) => content,
        Err(_) => return Ok(None),
    };

    Ok(Some(diff_preview(&path, &existing, &parsed.content)))
}

pub async fn execute(name: &str, args: &Value) -> Result<String> {
    Ok(execute_in_dir(name, args, None).await?.output)
}

pub async fn execute_in_dir(name: &str, args: &Value, current_cwd: Option<&Path>) -> Result<ToolExecution> {
    let cwd = match current_cwd {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir().context("failed to resolve current directory")?,
    };

    match name {
        "bash_exec" => {
            let parsed: BashExecArgs = serde_json::from_value(args.clone())?;
            let command_cwd = parsed
                .cwd
                .as_deref()
                .map(|dir| resolve_path(&cwd, dir))
                .unwrap_or(cwd);
            run_shell_command(&parsed.command, &command_cwd).await
        }
        "file_read" => {
            let parsed: FileReadArgs = serde_json::from_value(args.clone())?;
            let path = resolve_path(&cwd, &parsed.path);
            let contents = fs::read_to_string(&path)
                .await
                .with_context(|| format!("failed to read {}", path.display()))?;
            Ok(ToolExecution {
                output: truncate_lines(&contents, parsed.max_lines.unwrap_or(200) as usize),
                exit_code: Some(0),
                updated_cwd: None,
            })
        }
        "file_write" => {
            let parsed: FileWriteArgs = serde_json::from_value(args.clone())?;
            let path = resolve_path(&cwd, &parsed.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
            }
            fs::write(&path, parsed.content.as_bytes())
                .await
                .with_context(|| format!("failed to write {}", path.display()))?;

            Ok(ToolExecution {
                output: format!("Wrote {}", path.display()),
                exit_code: Some(0),
                updated_cwd: None,
            })
        }
        "git_status" => run_shell_command("git status --short", &cwd).await,
        other => Err(anyhow!("unknown tool: {other}")),
    }
}

async fn run_shell_command(command: &str, cwd: &Path) -> Result<ToolExecution> {
    let mut child = build_shell_command(command, cwd)?
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn command: {command}"))?;

    let stdout = child.stdout.take().context("missing stdout pipe")?;
    let stderr = child.stderr.take().context("missing stderr pipe")?;
    let (sender, mut receiver) = mpsc::unbounded_channel::<(bool, String)>();

    let stdout_sender = sender.clone();
    let stdout_task = tokio::spawn(async move {
        read_stream(stdout, false, stdout_sender).await
    });
    let stderr_task = tokio::spawn(async move {
        read_stream(stderr, true, sender).await
    });

    let mut output = String::new();
    let mut updated_cwd = None;
    while let Some((is_stderr, line)) = receiver.recv().await {
        if let Some(path) = line.strip_prefix(CWD_SENTINEL) {
            updated_cwd = Some(PathBuf::from(path.trim()));
            continue;
        }

        if is_stderr {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();

        output.push_str(&line);
        output.push('\n');
    }

    let status = child.wait().await.context("failed to wait for command")?;
    stdout_task.await.context("stdout reader task failed")??;
    stderr_task.await.context("stderr reader task failed")??;

    Ok(ToolExecution {
        output,
        exit_code: status.code(),
        updated_cwd,
    })
}

async fn read_stream<R>(
    reader: R,
    is_stderr: bool,
    sender: mpsc::UnboundedSender<(bool, String)>,
) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let _ = sender.send((is_stderr, line));
    }
    Ok(())
}

fn build_shell_command(command: &str, cwd: &Path) -> Result<Command> {
    #[cfg(target_os = "windows")]
    {
        let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        let wrapped = format!(
            "{command}\r\nset AGENTSH_EXIT=%ERRORLEVEL%\r\necho {CWD_SENTINEL}%CD%\r\nexit /b %AGENTSH_EXIT%"
        );
        let mut cmd = Command::new(shell);
        cmd.arg("/C").arg(wrapped).current_dir(cwd);
        Ok(cmd)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let wrapped = format!(
            "{command}\nstatus=$?\nprintf '{CWD_SENTINEL}%s\\n' \"$PWD\"\nexit $status"
        );
        let mut cmd = Command::new(shell);
        cmd.arg("-c").arg(wrapped).current_dir(cwd);
        Ok(cmd)
    }
}

fn resolve_path(base: &Path, raw_path: &str) -> PathBuf {
    let expanded = if raw_path == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw_path))
    } else if let Some(stripped) = raw_path.strip_prefix("~/") {
        dirs::home_dir()
            .map(|home| home.join(stripped))
            .unwrap_or_else(|| PathBuf::from(raw_path))
    } else {
        PathBuf::from(raw_path)
    };

    if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    }
}

fn truncate_lines(contents: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = contents.lines().collect();
    if lines.len() <= max_lines {
        return contents.to_string();
    }

    let mut truncated = lines[..max_lines].join("\n");
    truncated.push_str("\n[truncated]");
    truncated
}

fn diff_preview(path: &Path, existing: &str, updated: &str) -> String {
    if existing == updated {
        return "No content changes.".to_string();
    }

    let existing_lines: Vec<&str> = existing.lines().collect();
    let updated_lines: Vec<&str> = updated.lines().collect();
    let mut preview = vec![
        format!("--- {}", path.display()),
        format!("+++ {}", path.display()),
    ];
    let mut differences = 0usize;
    let max_lines = existing_lines.len().max(updated_lines.len());

    for index in 0..max_lines {
        let old_line = existing_lines.get(index).copied();
        let new_line = updated_lines.get(index).copied();
        if old_line == new_line {
            continue;
        }

        if let Some(line) = old_line {
            preview.push(format!("- {}", truncate_text(line, 72)));
        }
        if let Some(line) = new_line {
            preview.push(format!("+ {}", truncate_text(line, 72)));
        }

        differences += 1;
        if differences >= 6 {
            if index + 1 < max_lines {
                preview.push("...".to_string());
            }
            break;
        }
    }

    preview.join("\n")
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }

    let mut truncated: String = chars.into_iter().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}