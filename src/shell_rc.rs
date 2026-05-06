use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

const ACTIVATION_MARKER: &str = "AgentSH auto-activation";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RcKind {
    BashLike,
    Fish,
    PowerShell,
}

#[derive(Debug, Clone)]
struct RcTarget {
    path: PathBuf,
    kind: RcKind,
}

#[derive(Debug, Clone)]
pub struct DeactivationResult {
    pub path: PathBuf,
    pub removed: bool,
}

pub fn deactivate_for_current_shell() -> Result<Vec<DeactivationResult>> {
    let targets = rc_targets_for_current_shell()?;
    let mut results = Vec::with_capacity(targets.len());

    for target in targets {
        results.push(remove_activation_block(&target)?);
    }

    Ok(results)
}

pub fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = path.strip_prefix(&home) {
            if relative.as_os_str().is_empty() {
                return "~".to_string();
            }

            return format!("~/{}", relative.display());
        }
    }

    path.display().to_string()
}

fn rc_targets_for_current_shell() -> Result<Vec<RcTarget>> {
    let home = dirs::home_dir().context("failed to resolve home directory")?;
    let shell = env::var("SHELL").unwrap_or_default();
    let shell_name = Path::new(&shell)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    let targets = match shell_name {
        "bash" => vec![
            RcTarget {
                path: home.join(".bashrc"),
                kind: RcKind::BashLike,
            },
            RcTarget {
                path: home.join(".bash_profile"),
                kind: RcKind::BashLike,
            },
        ],
        "zsh" => vec![RcTarget {
            path: home.join(".zshrc"),
            kind: RcKind::BashLike,
        }],
        "fish" => vec![RcTarget {
            path: home.join(".config/fish/conf.d/agentsh.fish"),
            kind: RcKind::Fish,
        }],
        _ if cfg!(windows) => vec![RcTarget {
            path: powershell_profile_path(&home),
            kind: RcKind::PowerShell,
        }],
        _ => {
            return Err(anyhow!(
                "unsupported shell '{}'; remove the AgentSH auto-activation block manually",
                shell_name
            ))
        }
    };

    Ok(targets)
}

fn powershell_profile_path(home: &Path) -> PathBuf {
    let documents = home.join("Documents");
    let preferred = documents.join("PowerShell/Microsoft.PowerShell_profile.ps1");
    if preferred.exists() {
        preferred
    } else {
        documents.join("WindowsPowerShell/Microsoft.PowerShell_profile.ps1")
    }
}

fn remove_activation_block(target: &RcTarget) -> Result<DeactivationResult> {
    if !target.path.exists() {
        return Ok(DeactivationResult {
            path: target.path.clone(),
            removed: false,
        });
    }

    let contents = fs::read_to_string(&target.path)
        .with_context(|| format!("failed to read {}", target.path.display()))?;
    let Some(updated) = strip_block(&contents, target.kind) else {
        return Ok(DeactivationResult {
            path: target.path.clone(),
            removed: false,
        });
    };

    fs::write(&target.path, updated)
        .with_context(|| format!("failed to write {}", target.path.display()))?;

    Ok(DeactivationResult {
        path: target.path.clone(),
        removed: true,
    })
}

fn strip_block(contents: &str, kind: RcKind) -> Option<String> {
    let terminator = match kind {
        RcKind::BashLike => "fi",
        RcKind::Fish => "end",
        RcKind::PowerShell => "}",
    };

    let mut updated = String::with_capacity(contents.len());
    let mut skipping = false;
    let mut removed = false;

    for segment in contents.split_inclusive('\n') {
        let trimmed = segment.trim_end_matches(['\r', '\n']).trim();

        if skipping {
            if trimmed == terminator {
                skipping = false;
            }
            removed = true;
            continue;
        }

        if segment.contains(ACTIVATION_MARKER) {
            skipping = true;
            removed = true;
            if updated.ends_with("\n\n") {
                updated.pop();
            }
            continue;
        }

        updated.push_str(segment);
    }

    if !removed {
        return None;
    }

    while updated.contains("\n\n\n") {
        updated = updated.replace("\n\n\n", "\n\n");
    }

    Some(updated.trim_end_matches('\n').to_string() + "\n")
}

#[cfg(test)]
mod tests {
    use super::{strip_block, RcKind};

    #[test]
    fn strips_bash_activation_block() {
        let contents = "export PATH=\"$HOME/bin:$PATH\"\n\n# AgentSH auto-activation\nif [ -t 1 ] && \\\n+   [ -z \"$AGENTSH_ACTIVE\" ] && \\\n+   [ \"$TERM_PROGRAM\" != \"vscode\" ] && \\\n+   [ \"$TERM_PROGRAM\" != \"jetbrains\" ] && \\\n+   command -v agentsh > /dev/null 2>&1; then\n  exec agentsh\nfi\n\nalias ll='ls -la'\n";

        let stripped = strip_block(contents, RcKind::BashLike).expect("block removed");

        assert!(!stripped.contains("AgentSH auto-activation"));
        assert!(stripped.contains("alias ll='ls -la'"));
    }

    #[test]
    fn strips_fish_activation_block() {
        let contents = "set -gx PATH $HOME/bin $PATH\n\n# AgentSH auto-activation\nif status is-interactive\n    and test -z \"$AGENTSH_ACTIVE\"\n    and test \"$TERM_PROGRAM\" != \"vscode\"\n    and test \"$TERM_PROGRAM\" != \"jetbrains\"\n    and command -v agentsh > /dev/null 2>&1\n    exec agentsh\nend\n";

        let stripped = strip_block(contents, RcKind::Fish).expect("block removed");

        assert!(!stripped.contains("AgentSH auto-activation"));
        assert!(stripped.contains("set -gx PATH"));
    }
}