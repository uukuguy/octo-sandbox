//! V2 skill frontmatter parser.
//!
//! Parses the EAASP v2 skill frontmatter schema, which extends the legacy
//! schema with `runtime_affinity`, `access_scope`, `scoped_hooks`, and
//! `dependencies`. All fields are optional so legacy frontmatter parses
//! cleanly with default values.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct V2Frontmatter {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub runtime_affinity: RuntimeAffinity,
    #[serde(default)]
    pub access_scope: Option<String>,
    #[serde(default)]
    pub scoped_hooks: ScopedHooks,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RuntimeAffinity {
    #[serde(default)]
    pub preferred: Option<String>,
    #[serde(default)]
    pub compatible: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ScopedHooks {
    #[serde(default, rename = "PreToolUse")]
    pub pre_tool_use: Vec<ScopedHook>,
    #[serde(default, rename = "PostToolUse")]
    pub post_tool_use: Vec<ScopedHook>,
    #[serde(default, rename = "Stop")]
    pub stop: Vec<ScopedHook>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScopedHook {
    pub name: String,
    #[serde(flatten)]
    pub body: ScopedHookBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ScopedHookBody {
    Command { command: String },
    Prompt { prompt: String },
}

#[derive(Debug, Error)]
pub enum V2ParseError {
    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("empty frontmatter")]
    Empty,
}

/// Parse a v2 frontmatter YAML string.
///
/// Returns `V2ParseError::Empty` if the input is blank/whitespace-only, and
/// `V2ParseError::Yaml` for any serde parsing failure. Legacy frontmatter
/// (only `name`/`version`/`author`) parses successfully with default values
/// for the v2-specific fields.
pub fn parse_v2_frontmatter(yaml: &str) -> Result<V2Frontmatter, V2ParseError> {
    if yaml.trim().is_empty() {
        return Err(V2ParseError::Empty);
    }
    let fm: V2Frontmatter = serde_yaml::from_str(yaml)?;
    Ok(fm)
}

/// Runtime-provided directory variables that hook commands can reference.
///
/// Each field is a filesystem path string that will be substituted into
/// occurrences of `${SKILL_DIR}` / `${SESSION_DIR}` / `${RUNTIME_DIR}` inside
/// a hook's `command` or `prompt` body. Fields are optional because a runtime
/// may not know them yet (e.g. a test harness without a real session) — but
/// if the hook body references a variable whose value is `None`, substitution
/// fails fast instead of exec-ing a literal `${FOO}` as a path.
#[derive(Debug, Clone, Default)]
pub struct HookVars {
    pub skill_dir: Option<String>,
    pub session_dir: Option<String>,
    pub runtime_dir: Option<String>,
}

impl HookVars {
    pub fn with_skill_dir(skill_dir: impl Into<String>) -> Self {
        Self {
            skill_dir: Some(skill_dir.into()),
            session_dir: None,
            runtime_dir: None,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum HookSubstitutionError {
    #[error("unbound variable `${{{0}}}` — runtime did not provide a value")]
    Unbound(String),
    #[error("unknown variable `${{{0}}}` — allowed: SKILL_DIR, SESSION_DIR, RUNTIME_DIR")]
    Unknown(String),
    #[error("malformed variable reference near index {0} (unterminated `${{`)")]
    Malformed(usize),
}

/// Substitute `${SKILL_DIR}`, `${SESSION_DIR}`, `${RUNTIME_DIR}` in a hook body.
///
/// Other variable names fail with `Unknown`. An unbound (but known) variable
/// fails with `Unbound`. A malformed `${...` reference fails with `Malformed`.
/// Escape `$$` collapses to a literal `$` so commands can still reach real
/// shell variables after substitution.
pub fn substitute_hook_vars(
    input: &str,
    vars: &HookVars,
) -> Result<String, HookSubstitutionError> {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'$' {
            out.push('$');
            i += 2;
            continue;
        }
        if b == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let end = match input[i + 2..].find('}') {
                Some(rel) => i + 2 + rel,
                None => return Err(HookSubstitutionError::Malformed(i)),
            };
            let name = &input[i + 2..end];
            let value = match name {
                "SKILL_DIR" => vars.skill_dir.as_deref(),
                "SESSION_DIR" => vars.session_dir.as_deref(),
                "RUNTIME_DIR" => vars.runtime_dir.as_deref(),
                _ => return Err(HookSubstitutionError::Unknown(name.to_string())),
            };
            match value {
                Some(v) => out.push_str(v),
                None => return Err(HookSubstitutionError::Unbound(name.to_string())),
            }
            i = end + 1;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    Ok(out)
}

/// Walk every hook in `hooks` and apply substitution to its body, returning
/// a new `ScopedHooks` with resolved strings. Prompt hooks are substituted
/// too, so prompt text can reference `${SKILL_DIR}` for path hints.
pub fn substitute_scoped_hooks(
    hooks: &ScopedHooks,
    vars: &HookVars,
) -> Result<ScopedHooks, HookSubstitutionError> {
    let map = |list: &Vec<ScopedHook>| -> Result<Vec<ScopedHook>, HookSubstitutionError> {
        list.iter()
            .map(|h| {
                let body = match &h.body {
                    ScopedHookBody::Command { command } => ScopedHookBody::Command {
                        command: substitute_hook_vars(command, vars)?,
                    },
                    ScopedHookBody::Prompt { prompt } => ScopedHookBody::Prompt {
                        prompt: substitute_hook_vars(prompt, vars)?,
                    },
                };
                Ok(ScopedHook {
                    name: h.name.clone(),
                    body,
                })
            })
            .collect()
    };
    Ok(ScopedHooks {
        pre_tool_use: map(&hooks.pre_tool_use)?,
        post_tool_use: map(&hooks.post_tool_use)?,
        stop: map(&hooks.stop)?,
    })
}

#[cfg(test)]
mod hook_subst_tests {
    use super::*;

    fn vars_full() -> HookVars {
        HookVars {
            skill_dir: Some("/skills/threshold-calibration".into()),
            session_dir: Some("/var/session/abc".into()),
            runtime_dir: Some("/opt/grid-runtime".into()),
        }
    }

    #[test]
    fn substitutes_skill_dir() {
        let out = substitute_hook_vars(
            "${SKILL_DIR}/hooks/block_write_scada.sh",
            &vars_full(),
        )
        .unwrap();
        assert_eq!(out, "/skills/threshold-calibration/hooks/block_write_scada.sh");
    }

    #[test]
    fn substitutes_multiple_vars_in_one_string() {
        let out = substitute_hook_vars(
            "${RUNTIME_DIR}/bin/runner ${SKILL_DIR}/entry --session ${SESSION_DIR}",
            &vars_full(),
        )
        .unwrap();
        assert_eq!(
            out,
            "/opt/grid-runtime/bin/runner /skills/threshold-calibration/entry --session /var/session/abc"
        );
    }

    #[test]
    fn leaves_non_variable_text_untouched() {
        let out = substitute_hook_vars("/usr/bin/env bash", &vars_full()).unwrap();
        assert_eq!(out, "/usr/bin/env bash");
    }

    #[test]
    fn double_dollar_escapes_to_literal_dollar() {
        let out = substitute_hook_vars("echo $$HOME $${SKILL_DIR}", &vars_full()).unwrap();
        // `$$` → `$`, and the second `$$` also → `$` so the rest is literal.
        assert_eq!(out, "echo $HOME ${SKILL_DIR}");
    }

    #[test]
    fn unknown_variable_errors() {
        let err =
            substitute_hook_vars("${WHO_KNOWS}/x", &vars_full()).expect_err("must error");
        assert!(matches!(err, HookSubstitutionError::Unknown(name) if name == "WHO_KNOWS"));
    }

    #[test]
    fn unbound_known_variable_errors() {
        let vars = HookVars {
            skill_dir: None,
            session_dir: None,
            runtime_dir: None,
        };
        let err =
            substitute_hook_vars("${SKILL_DIR}/x", &vars).expect_err("must error");
        assert!(matches!(err, HookSubstitutionError::Unbound(name) if name == "SKILL_DIR"));
    }

    #[test]
    fn malformed_reference_errors() {
        let err = substitute_hook_vars("${SKILL_DIR/x", &vars_full()).expect_err("must error");
        assert!(matches!(err, HookSubstitutionError::Malformed(0)));
    }

    #[test]
    fn substitute_scoped_hooks_resolves_all_three_scopes() {
        let hooks = ScopedHooks {
            pre_tool_use: vec![ScopedHook {
                name: "pre".into(),
                body: ScopedHookBody::Command {
                    command: "${SKILL_DIR}/hooks/pre.sh".into(),
                },
            }],
            post_tool_use: vec![ScopedHook {
                name: "post".into(),
                body: ScopedHookBody::Prompt {
                    prompt: "Check outputs under ${SKILL_DIR}".into(),
                },
            }],
            stop: vec![ScopedHook {
                name: "stop".into(),
                body: ScopedHookBody::Command {
                    command: "${SKILL_DIR}/hooks/stop.sh".into(),
                },
            }],
        };
        let out = substitute_scoped_hooks(&hooks, &vars_full()).unwrap();
        match &out.pre_tool_use[0].body {
            ScopedHookBody::Command { command } => {
                assert_eq!(command, "/skills/threshold-calibration/hooks/pre.sh")
            }
            _ => panic!("expected command"),
        }
        match &out.post_tool_use[0].body {
            ScopedHookBody::Prompt { prompt } => {
                assert_eq!(prompt, "Check outputs under /skills/threshold-calibration")
            }
            _ => panic!("expected prompt"),
        }
        match &out.stop[0].body {
            ScopedHookBody::Command { command } => {
                assert_eq!(command, "/skills/threshold-calibration/hooks/stop.sh")
            }
            _ => panic!("expected command"),
        }
    }

    #[test]
    fn substitute_scoped_hooks_propagates_errors() {
        let hooks = ScopedHooks {
            pre_tool_use: vec![ScopedHook {
                name: "pre".into(),
                body: ScopedHookBody::Command {
                    command: "${NOPE}".into(),
                },
            }],
            ..Default::default()
        };
        let err =
            substitute_scoped_hooks(&hooks, &vars_full()).expect_err("must error");
        assert!(matches!(err, HookSubstitutionError::Unknown(name) if name == "NOPE"));
    }
}
