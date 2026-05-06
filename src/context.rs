use std::collections::{HashMap, VecDeque};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context as AnyhowContext, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub use crate::history::Turn;

const CWD_SENTINEL: &str = "__AGENTSH_CWD__=";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionMode {
    PerPlan,
    AutoApprove { countdown_secs: u8 },
}

pub struct Context {
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub scrollback: ScrollbackBuffer,
    pub turn_history: Vec<Turn>,
    pub permission_mode: PermissionMode,
}

pub struct ScrollbackBuffer {
    lines: VecDeque<String>,
    max_lines: usize,
}

pub struct PassthroughResult {
    pub output: String,
    pub exit_code: Option<i32>,
    pub interactive: bool,
}

impl Context {
    pub fn new(max_lines: usize, turn_history: Vec<Turn>) -> Result<Self> {
        let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
        let env = std::env::vars().collect();

        Ok(Self {
            cwd,
            env,
            scrollback: ScrollbackBuffer::new(max_lines),
            turn_history,
            permission_mode: PermissionMode::PerPlan,
        })
    }

    pub fn record_turn(&mut self, turn: Turn) {
        self.turn_history.push(turn);
    }

    pub fn shell(&self) -> String {
        #[cfg(unix)]
        {
            self.env
                .get("SHELL")
                .cloned()
                .unwrap_or_else(|| "/bin/sh".to_string())
        }

        #[cfg(target_os = "windows")]
        {
            self.env
                .get("COMSPEC")
                .cloned()
                .unwrap_or_else(|| "cmd.exe".to_string())
        }
    }
}

impl ScrollbackBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_lines.max(1)),
            max_lines: max_lines.max(1),
        }
    }

    pub fn push_line(&mut self, line: impl Into<String>) {
        if self.lines.len() == self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line.into());
    }

    pub fn push_text(&mut self, text: &str) {
        for line in text.lines() {
            self.push_line(line.to_string());
        }
    }

    pub fn render(&self) -> String {
        if self.lines.is_empty() {
            return "(none)".to_string();
        }

        self.lines.iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

pub async fn run_passthrough(command: &str, context: &mut Context) -> Result<PassthroughResult> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(PassthroughResult {
            output: String::new(),
            exit_code: Some(0),
            interactive: false,
        });
    }

    std::env::set_current_dir(&context.cwd)
        .with_context(|| format!("failed to switch into {}", context.cwd.display()))?;
    context.scrollback.push_line(format!("$ {trimmed}"));

    if requires_tty(trimmed) {
        let mut child = build_passthrough_command(trimmed, &context.cwd)?
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to spawn passthrough command: {trimmed}"))?;
        let status = child
            .wait()
            .await
            .context("failed while waiting for passthrough command")?;
        context.scrollback.push_line("[interactive command completed]");
        context.cwd = std::env::current_dir().context("failed to sync current directory")?;
        return Ok(PassthroughResult {
            output: "[interactive command completed]".to_string(),
            exit_code: status.code(),
            interactive: true,
        });
    }

    let mut child = build_passthrough_command(trimmed, &context.cwd)?
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn passthrough command: {trimmed}"))?;

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

    let mut updated_cwd = None;
    let mut output = String::new();
    while let Some((is_stderr, line)) = receiver.recv().await {
        if let Some(path) = line.strip_prefix(CWD_SENTINEL) {
            updated_cwd = Some(PathBuf::from(path.trim()));
            continue;
        }

        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&line);
        if is_stderr {
            let _ = io::stderr().flush();
        } else {
            let _ = io::stdout().flush();
        }
        context.scrollback.push_line(line);
    }

    let status = child
        .wait()
        .await
        .context("failed while waiting for passthrough command")?;
    stdout_task.await.context("stdout reader task failed")??;
    stderr_task.await.context("stderr reader task failed")??;

    if let Some(path) = updated_cwd {
        if path.exists() {
            std::env::set_current_dir(&path)
                .with_context(|| format!("failed to change directory to {}", path.display()))?;
        }
    }
    context.cwd = std::env::current_dir().context("failed to sync current directory")?;
    Ok(PassthroughResult {
        output,
        exit_code: status.code(),
        interactive: false,
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

fn requires_tty(command: &str) -> bool {
    let interactive = ["vim", "nano", "less", "top", "htop", "man"];
    command
        .split_whitespace()
        .next()
        .map(|first| interactive.contains(&first))
        .unwrap_or(false)
}

fn build_passthrough_command(command: &str, cwd: &PathBuf) -> Result<Command> {
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