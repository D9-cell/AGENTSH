use agentsh::parser::{classify, InputKind};

#[test]
fn detects_relative_path_commands() {
    assert_eq!(classify("./script.sh"), InputKind::DirectCommand);
}

#[test]
fn detects_absolute_path_commands() {
    assert_eq!(classify("/usr/bin/env"), InputKind::DirectCommand);
}

#[test]
fn detects_home_prefixed_commands() {
    assert_eq!(classify("~/bin/tool"), InputKind::DirectCommand);
}

#[test]
fn detects_known_shell_commands() {
    assert_eq!(classify("git status"), InputKind::DirectCommand);
    assert_eq!(classify("docker ps"), InputKind::DirectCommand);
}

#[test]
fn detects_shell_metacharacters() {
    assert_eq!(classify("ls | grep src"), InputKind::DirectCommand);
    assert_eq!(classify("echo hi > file.txt"), InputKind::DirectCommand);
    assert_eq!(classify("cargo test && cargo clippy"), InputKind::DirectCommand);
}

#[test]
fn treats_single_word_input_as_command() {
    assert_eq!(classify("htop"), InputKind::DirectCommand);
}

#[test]
fn treats_plain_language_as_natural_language() {
    assert_eq!(
        classify("set up a python venv and install flask"),
        InputKind::NaturalLanguage
    );
}

#[test]
fn ignores_empty_input_after_trim() {
    assert_eq!(classify("   "), InputKind::NaturalLanguage);
}