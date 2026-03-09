use octo_engine::skills::SkillSemanticIndex;

fn populated_index() -> SkillSemanticIndex {
    let mut index = SkillSemanticIndex::new();
    index.add_skill(
        "web-search",
        "Search the web for information and return results",
        &[
            "search".to_string(),
            "web".to_string(),
            "internet".to_string(),
        ],
    );
    index.add_skill(
        "file-manager",
        "Manage files on disk including read write and delete operations",
        &["files".to_string(), "io".to_string()],
    );
    index.add_skill(
        "code-runner",
        "Execute code snippets safely in a sandbox environment",
        &[
            "code".to_string(),
            "execution".to_string(),
            "sandbox".to_string(),
        ],
    );
    index.add_skill(
        "db-query",
        "Query databases using SQL and return structured results",
        &["database".to_string(), "sql".to_string()],
    );
    index
}

#[test]
fn test_index_new_is_empty() {
    let index = SkillSemanticIndex::new();
    assert!(index.is_empty());
    assert_eq!(index.len(), 0);
}

#[test]
fn test_index_default_is_empty() {
    let index = SkillSemanticIndex::default();
    assert!(index.is_empty());
}

#[test]
fn test_add_skill() {
    let mut index = SkillSemanticIndex::new();
    index.add_skill("test-skill", "A test skill", &["test".to_string()]);
    assert_eq!(index.len(), 1);
    assert!(!index.is_empty());
}

#[test]
fn test_remove_skill() {
    let mut index = populated_index();
    assert_eq!(index.len(), 4);
    index.remove_skill("web-search");
    assert_eq!(index.len(), 3);

    // Searching for "web" should not return web-search anymore
    let results = index.search("web", 10);
    assert!(results.iter().all(|m| m.skill_name != "web-search"));
}

#[test]
fn test_search_by_tag() {
    let index = populated_index();
    let results = index.search("search", 10);

    assert!(!results.is_empty());
    // web-search should be top result (tag match has higher weight)
    assert_eq!(results[0].skill_name, "web-search");
}

#[test]
fn test_search_by_description() {
    let index = populated_index();
    // "snippets" only appears in code-runner's description
    let results = index.search("snippets", 10);

    assert!(!results.is_empty());
    assert_eq!(results[0].skill_name, "code-runner");
}

#[test]
fn test_search_by_name_terms() {
    let index = populated_index();
    // "file" is a name term in "file-manager", should get highest weight
    let results = index.search("file", 10);

    assert!(!results.is_empty());
    assert_eq!(results[0].skill_name, "file-manager");
}

#[test]
fn test_search_no_results() {
    let index = populated_index();
    let results = index.search("blockchain", 10);
    assert!(results.is_empty());
}

#[test]
fn test_search_empty_query() {
    let index = populated_index();
    let results = index.search("", 10);
    assert!(results.is_empty());
}

#[test]
fn test_search_short_query_filtered() {
    let index = populated_index();
    // Single char query should be filtered out (< 2 chars)
    let results = index.search("a", 10);
    assert!(results.is_empty());
}

#[test]
fn test_search_with_limit() {
    let index = populated_index();
    // Search broadly to get multiple matches, then limit
    let results = index.search("the", 2);
    assert!(results.len() <= 2);
}

#[test]
fn test_search_results_sorted_by_score() {
    let index = populated_index();
    let results = index.search("code", 10);

    // Verify results are sorted by score descending
    for i in 1..results.len() {
        assert!(results[i - 1].score >= results[i].score);
    }
}

#[test]
fn test_search_matched_terms_populated() {
    let index = populated_index();
    let results = index.search("sql", 10);

    assert!(!results.is_empty());
    let top = &results[0];
    assert_eq!(top.skill_name, "db-query");
    assert!(!top.matched_terms.is_empty());
    assert!(top.matched_terms.contains(&"sql".to_string()));
}

#[test]
fn test_name_terms_have_highest_weight() {
    let mut index = SkillSemanticIndex::new();
    // Skill A: "search" only in description
    index.add_skill("skill-a", "search for things", &[]);
    // Skill B: "search" in name
    index.add_skill("search-tool", "a tool", &[]);

    let results = index.search("search", 10);
    assert!(results.len() >= 2);
    // search-tool should score higher due to name weight (5.0) vs description (1.0)
    assert_eq!(results[0].skill_name, "search-tool");
}

#[test]
fn test_tag_terms_have_medium_weight() {
    let mut index = SkillSemanticIndex::new();
    // Skill A: "code" only in description
    index.add_skill("skill-a", "code things here", &[]);
    // Skill B: "code" in tags
    index.add_skill("skill-b", "some tool", &["code".to_string()]);

    let results = index.search("code", 10);
    assert!(results.len() >= 2);
    // skill-b should score higher due to tag weight (3.0) vs description (1.0)
    assert_eq!(results[0].skill_name, "skill-b");
}
