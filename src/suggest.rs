use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Suggester {
    history: Vec<String>,
    builtin_completions: HashMap<String, Vec<String>>,
}

impl Suggester {
    pub fn new(history: Vec<String>) -> Self {
        let builtin_completions = HashMap::from([
            (
                "git ".to_string(),
                vec![
                    "status".to_string(),
                    "add -A".to_string(),
                    "commit -m \"\"".to_string(),
                    "push".to_string(),
                    "pull".to_string(),
                    "log --oneline".to_string(),
                    "diff".to_string(),
                    "checkout -b".to_string(),
                ],
            ),
            (
                "docker ".to_string(),
                vec![
                    "ps".to_string(),
                    "images".to_string(),
                    "run -it".to_string(),
                    "build -t".to_string(),
                    "compose up".to_string(),
                    "compose down".to_string(),
                ],
            ),
            (
                "cargo ".to_string(),
                vec![
                    "build".to_string(),
                    "run".to_string(),
                    "test".to_string(),
                    "clippy".to_string(),
                    "fmt".to_string(),
                    "add".to_string(),
                ],
            ),
            (
                "npm ".to_string(),
                vec![
                    "install".to_string(),
                    "run dev".to_string(),
                    "run build".to_string(),
                    "start".to_string(),
                    "test".to_string(),
                ],
            ),
            (
                "ls ".to_string(),
                vec!["-la".to_string(), "-lh".to_string(), "--color=auto".to_string()],
            ),
        ]);

        Self {
            history,
            builtin_completions,
        }
    }

    pub fn suggest(&self, input: &str) -> Option<String> {
        if input.trim().is_empty() {
            return None;
        }

        self.suggest_from_history(input)
            .or_else(|| self.suggest_from_builtins(input))
            .or_else(|| self.suggest_from_filesystem(input))
    }

    fn suggest_from_history(&self, input: &str) -> Option<String> {
        self.history
            .iter()
            .find(|command| command.starts_with(input) && command.as_str() != input)
            .cloned()
    }

    fn suggest_from_builtins(&self, input: &str) -> Option<String> {
        for prefix in ["git ", "docker ", "cargo ", "npm ", "ls "] {
            if !input.starts_with(prefix) {
                continue;
            }

            let remainder = &input[prefix.len()..];
            let completions = self.builtin_completions.get(prefix)?;
            for completion in completions {
                if completion.starts_with(remainder) {
                    let suggestion = format!("{prefix}{completion}");
                    if suggestion != input {
                        return Some(suggestion);
                    }
                }
            }
        }

        None
    }

    fn suggest_from_filesystem(&self, input: &str) -> Option<String> {
        let token_start = input
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(index, ch)| index + ch.len_utf8())
            .unwrap_or(0);
        let token = &input[token_start..];
        if token.is_empty() || !looks_like_path(token) {
            return None;
        }

        let resolved_token = resolve_path_token(token)?;
        let (search_dir, partial_name) = search_directory_and_prefix(&resolved_token, token.ends_with('/'))?;
        let mut matches = fs::read_dir(&search_dir)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                if !file_name.starts_with(&partial_name) {
                    return None;
                }

                let is_dir = entry.file_type().ok()?.is_dir();
                Some((file_name, is_dir))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.0.cmp(&right.0));

        let (match_name, is_dir) = matches.into_iter().next()?;
        let suggestion_suffix = if is_dir {
            format!("{match_name}/")
        } else {
            match_name
        };

        let suggestion_token = if token.ends_with('/') {
            format!("{token}{suggestion_suffix}")
        } else if let Some((prefix, _)) = token.rsplit_once('/') {
            format!("{prefix}/{suggestion_suffix}")
        } else {
            suggestion_suffix
        };

        let suggestion = format!("{}{}", &input[..token_start], suggestion_token);
        (suggestion != input).then_some(suggestion)
    }
}

fn looks_like_path(token: &str) -> bool {
    token.contains('/') || token.starts_with('.') || token.starts_with('~')
}

fn resolve_path_token(token: &str) -> Option<PathBuf> {
    if let Some(stripped) = token.strip_prefix("~/") {
        return dirs::home_dir().map(|home| home.join(stripped));
    }

    if token == "~" {
        return dirs::home_dir();
    }

    Some(PathBuf::from(token))
}

fn search_directory_and_prefix(path: &Path, token_ends_with_separator: bool) -> Option<(PathBuf, String)> {
    if token_ends_with_separator {
        return Some((path.to_path_buf(), String::new()));
    }

    let partial_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let search_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Some((search_dir, partial_name))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::Suggester;

    #[test]
    fn prefers_history_matches() {
        let suggester = Suggester::new(vec!["git stash".to_string(), "git status".to_string()]);
        assert_eq!(suggester.suggest("git st"), Some("git stash".to_string()));
    }

    #[test]
    fn falls_back_to_builtin_completions() {
        let suggester = Suggester::new(Vec::new());
        assert_eq!(suggester.suggest("git st"), Some("git status".to_string()));
    }

    #[test]
    fn completes_partial_filesystem_paths() {
        let original_cwd = std::env::current_dir().unwrap();
        let temp_dir = unique_temp_dir();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let suggester = Suggester::new(Vec::new());
        assert_eq!(suggester.suggest("cat src/ma"), Some("cat src/main.rs".to_string()));

        std::env::set_current_dir(original_cwd).unwrap();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn skips_exact_matches() {
        let suggester = Suggester::new(vec!["cargo test".to_string()]);
        assert_eq!(suggester.suggest("cargo test"), None);
    }

    fn unique_temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("agentsh-suggest-{nanos}"))
    }
}