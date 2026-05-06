use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Safe,
    High,
    Critical,
}

fn patterns() -> &'static [(Regex, RiskLevel)] {
    static PATTERNS: OnceLock<Vec<(Regex, RiskLevel)>> = OnceLock::new();

    PATTERNS.get_or_init(|| {
        [
            (r"rm\s+-rf", RiskLevel::Critical),
            (r"dd\s+if=", RiskLevel::Critical),
            (r"mkfs\.", RiskLevel::Critical),
            (r">\s*/dev/sd", RiskLevel::Critical),
            (r">\s*/dev/nvme", RiskLevel::Critical),
            (r"shred\s+", RiskLevel::Critical),
            (r"chmod\s+777", RiskLevel::High),
            (r"sudo\s+", RiskLevel::High),
            (r"curl.+\|\s*(bash|sh)", RiskLevel::High),
            (r"wget.+\|\s*(bash|sh)", RiskLevel::High),
            (r"DROP\s+TABLE", RiskLevel::High),
            (r"truncate\s+", RiskLevel::High),
            (r"(?:pkill|killall)\b", RiskLevel::High),
        ]
        .into_iter()
        .map(|(pattern, level)| {
            let regex = RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .expect("valid safety regex");
            (regex, level)
        })
        .collect()
    })
}

pub fn score(command: &str) -> RiskLevel {
    let mut risk = RiskLevel::Safe;

    for (pattern, level) in patterns() {
        if pattern.is_match(command) && *level > risk {
            risk = *level;
        }
    }

    risk
}