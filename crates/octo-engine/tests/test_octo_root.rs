//! Integration tests for OctoRoot unified directory management.

use std::path::Path;

use octo_engine::root::{encode_project_key, OctoRoot};

#[test]
fn test_full_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let global = tmp.path().join("dot-octo");
    let project = tmp.path().join("project-root");
    let working = tmp.path().join("my-project");
    std::fs::create_dir_all(&working).unwrap();

    std::env::set_var("OCTO_GLOBAL_ROOT", &global);
    std::env::set_var("OCTO_PROJECT_ROOT", &project);
    std::env::remove_var("OCTO_DB_PATH");

    let root = OctoRoot::with_working_dir(&working).unwrap();

    // Verify path accessors
    assert_eq!(root.global_root(), global.as_path());
    assert_eq!(root.project_root(), project.as_path());
    assert_eq!(root.working_dir(), working.as_path());

    // Ensure directories
    root.ensure_dirs().unwrap();

    // All directories should exist
    assert!(root.project_data_dir().is_dir());
    assert!(root.history_dir().is_dir());
    assert!(root.cache_dir().is_dir());
    assert!(root.global_skills_dir().is_dir());
    assert!(root.project_skills_dir().is_dir());

    // meta.json should exist and contain path
    let meta_path = root.project_meta_path();
    assert!(meta_path.exists());
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    let recorded_path = meta["path"].as_str().unwrap();
    assert!(
        recorded_path.contains("my-project"),
        "meta.json path should contain working dir name, got: {}",
        recorded_path
    );
    assert!(meta["created_at"].is_string());

    // Calling ensure_dirs again should NOT overwrite meta.json
    let original_meta = std::fs::read_to_string(&meta_path).unwrap();
    root.ensure_dirs().unwrap();
    let second_meta = std::fs::read_to_string(&meta_path).unwrap();
    assert_eq!(original_meta, second_meta, "meta.json should not be overwritten");

    // DB path should be under project data dir
    let db = root.db_path();
    assert!(db.starts_with(root.project_data_dir()));
    assert!(db.to_string_lossy().ends_with("octo.db"));

    // resolve_db_path should return new default (no legacy files)
    assert_eq!(root.resolve_db_path(), root.db_path());

    // Clean up env
    std::env::remove_var("OCTO_GLOBAL_ROOT");
    std::env::remove_var("OCTO_PROJECT_ROOT");
}

#[test]
fn test_skills_dir_structure() {
    let tmp = tempfile::tempdir().unwrap();
    let global = tmp.path().join("global");
    let project = tmp.path().join("project");

    std::env::set_var("OCTO_GLOBAL_ROOT", &global);
    std::env::set_var("OCTO_PROJECT_ROOT", &project);

    let root = OctoRoot::with_working_dir(tmp.path()).unwrap();

    // skills_dirs returns [project, global]
    let dirs = root.skills_dirs();
    assert_eq!(dirs.len(), 2);
    assert_eq!(dirs[0], project.join("skills"));
    assert_eq!(dirs[1], global.join("skills"));

    std::env::remove_var("OCTO_GLOBAL_ROOT");
    std::env::remove_var("OCTO_PROJECT_ROOT");
}

#[test]
fn test_path_encoding_edge_cases() {
    // Simple path
    assert_eq!(
        encode_project_key(Path::new("/Users/foo/bar")),
        "Users_foo_bar"
    );

    // Root path
    assert_eq!(encode_project_key(Path::new("/")), "");

    // Nested deep path
    let deep = "/a/b/c/d/e/f/g/h/i/j";
    let key = encode_project_key(Path::new(deep));
    assert_eq!(key, "a_b_c_d_e_f_g_h_i_j");

    // Very long path (>200 chars) gets truncated with hash
    let segments: Vec<String> = (0..50).map(|i| format!("segment{}", i)).collect();
    let long_path = format!("/{}", segments.join("/"));
    let key = encode_project_key(Path::new(&long_path));
    assert!(key.len() < 200, "Encoded key should be <200 chars: {}", key.len());
    // Should contain underscore separating prefix from hash
    assert!(key.chars().filter(|c| *c == '_').count() >= 1);

    // Relative path (no leading /)
    assert_eq!(
        encode_project_key(Path::new("relative/path")),
        "relative_path"
    );

    // Path with spaces
    let key = encode_project_key(Path::new("/Users/foo bar/my project"));
    assert!(key.contains("foo bar"));
    assert!(key.contains("my project"));
}

#[test]
fn test_env_override_db_path() {
    let tmp = tempfile::tempdir().unwrap();
    let custom_db = tmp.path().join("custom").join("my.db");

    std::env::set_var("OCTO_DB_PATH", &custom_db);
    std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("g"));

    let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
    assert_eq!(root.resolve_db_path(), custom_db);

    std::env::remove_var("OCTO_DB_PATH");
    std::env::remove_var("OCTO_GLOBAL_ROOT");
}
