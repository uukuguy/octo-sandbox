use eaasp_skill_registry::models::{SkillStatus, SubmitDraftRequest};
use eaasp_skill_registry::store::SkillStore;

fn make_draft(id: &str, name: &str, version: &str, tags: Vec<&str>) -> SubmitDraftRequest {
    SubmitDraftRequest {
        id: id.to_string(),
        name: name.to_string(),
        description: format!("{name} description"),
        version: version.to_string(),
        author: Some("tester".to_string()),
        tags: Some(tags.into_iter().map(String::from).collect()),
        frontmatter_yaml: format!("name: {name}\nversion: {version}\n"),
        prose: format!("# {name}\n\nThis is the prose for {name}."),
    }
}

#[tokio::test]
async fn store_submit_and_read() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SkillStore::open(tmp.path()).await.unwrap();

    let req = make_draft(
        "hello-skill",
        "Hello Skill",
        "0.1.0",
        vec!["greeting", "demo"],
    );
    let meta = store.submit_draft(req).await.unwrap();

    assert_eq!(meta.id, "hello-skill");
    assert_eq!(meta.name, "Hello Skill");
    assert_eq!(meta.status, SkillStatus::Draft);
    assert_eq!(meta.tags, vec!["greeting", "demo"]);

    // Read back
    let content = store
        .read_skill("hello-skill".to_string(), Some("0.1.0".to_string()))
        .await
        .unwrap()
        .expect("skill should exist");

    assert_eq!(content.meta.id, "hello-skill");
    assert_eq!(content.meta.version, "0.1.0");
    assert!(content.prose.contains("This is the prose for Hello Skill"));
    assert!(content.frontmatter_yaml.contains("name: Hello Skill"));
}

#[tokio::test]
async fn store_search_by_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SkillStore::open(tmp.path()).await.unwrap();

    store
        .submit_draft(make_draft("alpha", "Alpha", "1.0.0", vec!["code", "rust"]))
        .await
        .unwrap();
    store
        .submit_draft(make_draft("beta", "Beta", "1.0.0", vec!["code", "python"]))
        .await
        .unwrap();

    // Search by tag "rust"
    let results = store
        .search(Some("rust".to_string()), None, None, None, None)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "alpha");

    // Search by tag "code" should return both
    let results = store
        .search(Some("code".to_string()), None, None, None, None)
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn store_promote_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SkillStore::open(tmp.path()).await.unwrap();

    store
        .submit_draft(make_draft("lifecycle", "Lifecycle", "1.0.0", vec!["test"]))
        .await
        .unwrap();

    // Draft -> Tested
    store
        .promote(
            "lifecycle".to_string(),
            "1.0.0".to_string(),
            SkillStatus::Tested,
        )
        .await
        .unwrap();
    let content = store
        .read_skill("lifecycle".to_string(), Some("1.0.0".to_string()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(content.meta.status, SkillStatus::Tested);

    // Tested -> Reviewed
    store
        .promote(
            "lifecycle".to_string(),
            "1.0.0".to_string(),
            SkillStatus::Reviewed,
        )
        .await
        .unwrap();
    let content = store
        .read_skill("lifecycle".to_string(), Some("1.0.0".to_string()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(content.meta.status, SkillStatus::Reviewed);

    // Reviewed -> Production
    store
        .promote(
            "lifecycle".to_string(),
            "1.0.0".to_string(),
            SkillStatus::Production,
        )
        .await
        .unwrap();
    let content = store
        .read_skill("lifecycle".to_string(), Some("1.0.0".to_string()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(content.meta.status, SkillStatus::Production);

    // Verify versions list
    let versions = store.list_versions("lifecycle".to_string()).await.unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].status, SkillStatus::Production);
}
