//! Builtin Skills Initializer — syncs embedded skills to `.octo/skills/` on startup.
//!
//! Compares SHA256 of embedded SKILL.md with on-disk version.
//! Only overwrites when the content differs (version update).

use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};
use tracing::{debug, info};

/// Embedded builtin skill: name and SKILL.md content.
struct BuiltinSkill {
    name: &'static str,
    content: &'static str,
}

/// All builtin skills compiled into the binary.
const BUILTIN_SKILLS: &[BuiltinSkill] = &[
    BuiltinSkill {
        name: "filesystem",
        content: include_str!("../../builtin/skills/filesystem/SKILL.md"),
    },
    BuiltinSkill {
        name: "web-search",
        content: include_str!("../../builtin/skills/web-search/SKILL.md"),
    },
    BuiltinSkill {
        name: "code-review",
        content: include_str!("../../builtin/skills/code-review/SKILL.md"),
    },
    BuiltinSkill {
        name: "code-debugger",
        content: include_str!("../../builtin/skills/code-debugger/SKILL.md"),
    },
    BuiltinSkill {
        name: "readme-writer",
        content: include_str!("../../builtin/skills/readme-writer/SKILL.md"),
    },
];

/// Sync builtin skills to the target directory.
///
/// For each builtin skill:
/// 1. Create `<target_dir>/<name>/SKILL.md` if missing
/// 2. If it exists, compare SHA256 — overwrite only if content changed
///
/// Returns the number of skills synced (created or updated).
pub fn sync_builtin_skills(target_dir: &Path) -> Result<usize> {
    let mut synced = 0;

    for skill in BUILTIN_SKILLS {
        let skill_dir = target_dir.join(skill.name);
        let skill_file = skill_dir.join("SKILL.md");

        if skill_file.exists() {
            // Compare SHA256
            let existing = std::fs::read_to_string(&skill_file)?;
            let existing_hash = sha256_hex(&existing);
            let builtin_hash = sha256_hex(skill.content);

            if existing_hash == builtin_hash {
                debug!(name = skill.name, "Builtin skill unchanged, skipping");
                continue;
            }

            info!(
                name = skill.name,
                "Builtin skill updated, overwriting"
            );
        } else {
            info!(name = skill.name, "Installing builtin skill");
        }

        // Create directory and write file
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(&skill_file, skill.content)?;
        synced += 1;
    }

    if synced > 0 {
        info!(count = synced, "Builtin skills synced");
    } else {
        debug!("All builtin skills up to date");
    }

    Ok(synced)
}

/// Get the list of builtin skill names.
pub fn builtin_skill_names() -> Vec<&'static str> {
    BUILTIN_SKILLS.iter().map(|s| s.name).collect()
}

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_skills_count() {
        assert_eq!(BUILTIN_SKILLS.len(), 5);
    }

    #[test]
    fn test_builtin_skill_names() {
        let names = builtin_skill_names();
        assert!(names.contains(&"filesystem"));
        assert!(names.contains(&"web-search"));
        assert!(names.contains(&"code-review"));
        assert!(names.contains(&"code-debugger"));
        assert!(names.contains(&"readme-writer"));
    }

    #[test]
    fn test_sync_to_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let count = sync_builtin_skills(dir.path()).unwrap();
        assert_eq!(count, 5);

        // Verify files exist
        for skill in BUILTIN_SKILLS {
            let path = dir.path().join(skill.name).join("SKILL.md");
            assert!(path.exists(), "Missing: {}", path.display());
        }
    }

    #[test]
    fn test_sync_idempotent() {
        let dir = tempfile::tempdir().unwrap();

        // First sync
        let count1 = sync_builtin_skills(dir.path()).unwrap();
        assert_eq!(count1, 5);

        // Second sync — should be no-op
        let count2 = sync_builtin_skills(dir.path()).unwrap();
        assert_eq!(count2, 0);
    }

    #[test]
    fn test_sync_detects_changes() {
        let dir = tempfile::tempdir().unwrap();

        // First sync
        sync_builtin_skills(dir.path()).unwrap();

        // Modify one file
        let path = dir.path().join("filesystem").join("SKILL.md");
        std::fs::write(&path, "modified content").unwrap();

        // Second sync — should update the modified one
        let count = sync_builtin_skills(dir.path()).unwrap();
        assert_eq!(count, 1);
    }
}
