use octo_types::skill::{SkillDefinition, SkillSourceType, TrustLevel};

/// Read-only tool whitelist for Unknown trust level.
const READONLY_TOOLS: &[&str] = &[
    "read",
    "file_read",
    "glob",
    "grep",
    "list_directory",
    "find",
];

/// TrustManager implements IronClaw-style Trust Attenuation.
///
/// Trust is determined by min(declared_trust, source_inferred_trust):
/// - ProjectLocal  -> can be Trusted
/// - UserLocal     -> capped at Installed
/// - PluginBundled -> capped at Installed
/// - Registry      -> capped at Unknown
pub struct TrustManager {
    /// Additional tools considered safe for Unknown trust level.
    extra_readonly_tools: Vec<String>,
}

impl TrustManager {
    pub fn new(extra_readonly_tools: Vec<String>) -> Self {
        Self {
            extra_readonly_tools,
        }
    }

    /// Compute effective trust level: min(declared, source-inferred).
    pub fn effective_trust_level(&self, skill: &SkillDefinition) -> TrustLevel {
        let source_max = match skill.source_type {
            SkillSourceType::ProjectLocal => TrustLevel::Trusted,
            SkillSourceType::UserLocal => TrustLevel::Installed,
            SkillSourceType::PluginBundled => TrustLevel::Installed,
            SkillSourceType::Registry => TrustLevel::Unknown,
        };
        // Take the more restrictive (lower) of declared and source-inferred.
        Self::min_trust(&skill.trust_level, &source_max)
    }

    /// Check if a tool is permitted for the given skill's trust level.
    pub fn check_tool_permission(
        &self,
        skill: &SkillDefinition,
        tool_name: &str,
    ) -> Result<(), TrustError> {
        let effective = self.effective_trust_level(skill);

        // Check denied_tools first (always enforced).
        if let Some(ref denied) = skill.denied_tools {
            if denied.iter().any(|d| d == tool_name) {
                return Err(TrustError::DeniedTool {
                    tool: tool_name.to_string(),
                    skill: skill.name.clone(),
                });
            }
        }

        match effective {
            TrustLevel::Trusted => Ok(()), // All tools allowed.
            TrustLevel::Installed => {
                // Only allowed_tools list.
                match &skill.allowed_tools {
                    Some(allowed) => {
                        if allowed.iter().any(|a| a == "*" || a == tool_name) {
                            Ok(())
                        } else {
                            Err(TrustError::NotInAllowedList {
                                tool: tool_name.to_string(),
                                skill: skill.name.clone(),
                            })
                        }
                    }
                    None => {
                        // No allowed_tools specified for Installed = deny all.
                        Err(TrustError::NoAllowedToolsDefined {
                            skill: skill.name.clone(),
                        })
                    }
                }
            }
            TrustLevel::Unknown => {
                // Read-only tools only.
                if self.is_readonly_tool(tool_name) {
                    Ok(())
                } else {
                    Err(TrustError::UnknownTrustReadOnly {
                        tool: tool_name.to_string(),
                        skill: skill.name.clone(),
                    })
                }
            }
        }
    }

    fn is_readonly_tool(&self, tool_name: &str) -> bool {
        READONLY_TOOLS.contains(&tool_name)
            || self.extra_readonly_tools.iter().any(|t| t == tool_name)
    }

    fn min_trust(a: &TrustLevel, b: &TrustLevel) -> TrustLevel {
        let rank = |t: &TrustLevel| match t {
            TrustLevel::Trusted => 2,
            TrustLevel::Installed => 1,
            TrustLevel::Unknown => 0,
        };
        if rank(a) <= rank(b) {
            a.clone()
        } else {
            b.clone()
        }
    }
}

impl Default for TrustManager {
    fn default() -> Self {
        Self::new(vec![])
    }
}

/// Errors from trust checking.
#[derive(Debug, Clone, PartialEq)]
pub enum TrustError {
    DeniedTool { tool: String, skill: String },
    NotInAllowedList { tool: String, skill: String },
    NoAllowedToolsDefined { skill: String },
    UnknownTrustReadOnly { tool: String, skill: String },
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeniedTool { tool, skill } => {
                write!(
                    f,
                    "Tool '{}' is explicitly denied by skill '{}'",
                    tool, skill
                )
            }
            Self::NotInAllowedList { tool, skill } => {
                write!(
                    f,
                    "Tool '{}' is not in allowed_tools for skill '{}'",
                    tool, skill
                )
            }
            Self::NoAllowedToolsDefined { skill } => {
                write!(
                    f,
                    "No allowed_tools defined for Installed-trust skill '{}'",
                    skill
                )
            }
            Self::UnknownTrustReadOnly { tool, skill } => {
                write!(
                    f,
                    "Tool '{}' blocked: skill '{}' has Unknown trust (read-only only)",
                    tool, skill
                )
            }
        }
    }
}

impl std::error::Error for TrustError {}
