//! Builtin Skills Initializer — seeds embedded skills to `.octo/skills/` on startup.
//!
//! Only writes a builtin skill when the skill directory does NOT exist on disk.
//! If a user has placed or modified a skill in `.octo/skills/`, it is never overwritten.

use std::path::Path;

use anyhow::Result;
use tracing::{debug, info};

/// Embedded builtin skill: name and SKILL.md content.
struct BuiltinSkill {
    name: &'static str,
    content: &'static str,
}

/// Minimal builtin skills compiled into the binary as fallback.
/// Only seeded when `.octo/skills/` doesn't have them yet.
/// The full set of skills lives in `.octo/skills/` (git tracked, user-managed).
const BUILTIN_SKILLS: &[BuiltinSkill] = &[
    BuiltinSkill {
        name: "filesystem",
        content: include_str!("../../builtin/skills/filesystem/SKILL.md"),
    },
    BuiltinSkill {
        name: "web-search",
        content: include_str!("../../builtin/skills/web-search/SKILL.md"),
    },
];

/// Seed builtin skills to the target directory (fallback for fresh projects).
///
/// For each builtin skill:
/// - If `<target_dir>/<name>/` does NOT exist → create it and write SKILL.md
/// - If it already exists → skip entirely (never overwrite user files)
///
/// Returns the number of skills seeded.
pub fn sync_builtin_skills(target_dir: &Path) -> Result<usize> {
    let mut synced = 0;

    for skill in BUILTIN_SKILLS {
        let skill_dir = target_dir.join(skill.name);

        if skill_dir.exists() {
            debug!(name = skill.name, "Skill directory exists, skipping");
            continue;
        }

        info!(name = skill.name, "Seeding builtin skill");
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(skill_dir.join("SKILL.md"), skill.content)?;
        synced += 1;
    }

    if synced > 0 {
        info!(count = synced, "Builtin skills seeded");
    } else {
        debug!("All builtin skills already present on disk");
    }

    Ok(synced)
}

/// Get the list of builtin skill names.
pub fn builtin_skill_names() -> Vec<&'static str> {
    BUILTIN_SKILLS.iter().map(|s| s.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_skills_count() {
        assert_eq!(BUILTIN_SKILLS.len(), 2);
    }

    #[test]
    fn test_builtin_skill_names() {
        let names = builtin_skill_names();
        assert!(names.contains(&"filesystem"));
        assert!(names.contains(&"web-search"));
    }

    #[test]
    fn test_sync_to_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let count = sync_builtin_skills(dir.path()).unwrap();
        assert_eq!(count, 2);

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
        assert_eq!(count1, 2);

        // Second sync — should be no-op
        let count2 = sync_builtin_skills(dir.path()).unwrap();
        assert_eq!(count2, 0);
    }

    #[test]
    fn test_sync_does_not_overwrite_user_files() {
        let dir = tempfile::tempdir().unwrap();

        // First sync
        sync_builtin_skills(dir.path()).unwrap();

        // Modify one file
        let path = dir.path().join("filesystem").join("SKILL.md");
        std::fs::write(&path, "user modified content").unwrap();

        // Second sync — should NOT overwrite (directory exists)
        let count = sync_builtin_skills(dir.path()).unwrap();
        assert_eq!(count, 0);

        // Verify user's modification is preserved
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "user modified content");
    }
}
