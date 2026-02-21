use std::io::IsTerminal;

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub is_agent: bool,
    pub detected_by: Option<String>,
}

pub fn detect(force_agent: bool, json_explicit: bool) -> AgentInfo {
    // 1. Explicit --agent flag
    if force_agent {
        return AgentInfo {
            is_agent: true,
            detected_by: Some("--agent flag".to_string()),
        };
    }

    // 2. Known LLM agent environment variables
    let env_checks = [
        ("CLAUDE_CODE", None),
        ("CURSOR_SESSION", None),
        ("CODEX", None),
        ("CONTINUE_SESSION", None),
        ("AIDER_SESSION", None),
        ("LLM_AGENT", Some("1")),
    ];

    for (var, expected_val) in &env_checks {
        if let Ok(val) = std::env::var(var) {
            if expected_val.is_none() || expected_val == &Some(val.as_str()) {
                return AgentInfo {
                    is_agent: true,
                    detected_by: Some(format!("env {}", var)),
                };
            }
        }
    }

    // CI=true combined with no TTY
    if std::env::var("CI").is_ok_and(|v| v == "true") && !std::io::stdin().is_terminal() {
        return AgentInfo {
            is_agent: true,
            detected_by: Some("CI=true + no TTY".to_string()),
        };
    }

    // 3. Parent process tree check
    if let Some(detected_by) = check_parent_processes() {
        return AgentInfo {
            is_agent: true,
            detected_by: Some(detected_by),
        };
    }

    // 4. TTY check: stdin not a terminal and --json not explicitly passed
    if !std::io::stdin().is_terminal() && !json_explicit {
        return AgentInfo {
            is_agent: true,
            detected_by: Some("no TTY on stdin".to_string()),
        };
    }

    AgentInfo {
        is_agent: false,
        detected_by: None,
    }
}

fn check_parent_processes() -> Option<String> {
    use sysinfo::{Pid, System};

    let known_agents = ["claude", "cursor", "codex", "aider", "continue"];
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut current_pid = Pid::from_u32(std::process::id());

    // Walk up to 10 levels of parent processes
    for _ in 0..10 {
        let process = sys.process(current_pid)?;
        let name = process.name().to_string_lossy().to_lowercase();

        for agent in &known_agents {
            if name.contains(agent) {
                return Some(format!("parent process: {}", name));
            }
        }

        current_pid = process.parent()?;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_force_agent_flag() {
        let info = detect(true, false);
        assert!(info.is_agent);
        assert_eq!(info.detected_by.as_deref(), Some("--agent flag"));
    }
}
