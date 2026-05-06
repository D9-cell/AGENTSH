#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    DirectCommand,
    NaturalLanguage,
}

const KNOWN_COMMANDS: &[&str] = &[
    "cd",
    "ls",
    "cat",
    "grep",
    "echo",
    "export",
    "source",
    "alias",
    "exit",
    "pwd",
    "mkdir",
    "rm",
    "mv",
    "cp",
    "touch",
    "chmod",
    "sudo",
    "git",
    "docker",
    "npm",
    "cargo",
    "python",
    "pip",
    "make",
    "curl",
    "wget",
    "ssh",
    "scp",
    "tar",
    "zip",
    "unzip",
    "vim",
    "nano",
];

pub fn classify(input: &str) -> InputKind {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return InputKind::NaturalLanguage;
    }

    if trimmed.starts_with("./") || trimmed.starts_with('/') || trimmed.starts_with('~') {
        return InputKind::DirectCommand;
    }

    let mut tokens = trimmed.split_whitespace();
    if let Some(first) = tokens.next() {
        if KNOWN_COMMANDS.contains(&first) {
            return InputKind::DirectCommand;
        }
    }

    if trimmed
        .split_whitespace()
        .any(|token| token == "|" || token == ">" || token == "<" || token == "&&" || token == "||" || token == ";")
    {
        return InputKind::DirectCommand;
    }

    if !trimmed.contains(' ') {
        return InputKind::DirectCommand;
    }

    InputKind::NaturalLanguage
}