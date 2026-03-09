use octo_engine::skills::{CatalogEntry, CatalogQuery, SkillCatalog};

fn make_entry(id: &str, name: &str, desc: &str, tags: &[&str], runtime: &str, downloads: u64) -> CatalogEntry {
    CatalogEntry {
        id: id.to_string(),
        name: name.to_string(),
        description: desc.to_string(),
        version: "1.0.0".to_string(),
        author: Some("test-author".to_string()),
        tags: tags.iter().map(|t| t.to_string()).collect(),
        url: None,
        checksum: None,
        runtime: Some(runtime.to_string()),
        downloads,
    }
}

fn populated_catalog() -> SkillCatalog {
    let mut catalog = SkillCatalog::new();
    catalog.add_entry(make_entry("org/web-search", "Web Search", "Search the web for information", &["search", "web"], "python", 1000));
    catalog.add_entry(make_entry("org/file-manager", "File Manager", "Manage files on disk", &["files", "io"], "nodejs", 500));
    catalog.add_entry(make_entry("org/code-runner", "Code Runner", "Execute code snippets safely", &["code", "execution"], "wasm", 2000));
    catalog.add_entry(make_entry("org/db-query", "DB Query", "Query databases with SQL", &["database", "sql"], "python", 800));
    catalog
}

#[test]
fn test_catalog_new_is_empty() {
    let catalog = SkillCatalog::new();
    assert!(catalog.is_empty());
    assert_eq!(catalog.len(), 0);
    assert!(!catalog.has_registry());
}

#[test]
fn test_catalog_default_is_empty() {
    let catalog = SkillCatalog::default();
    assert!(catalog.is_empty());
}

#[test]
fn test_catalog_add_and_get() {
    let mut catalog = SkillCatalog::new();
    let entry = make_entry("org/test", "Test Skill", "A test skill", &["test"], "python", 100);
    catalog.add_entry(entry);

    assert_eq!(catalog.len(), 1);
    assert!(!catalog.is_empty());

    let retrieved = catalog.get("org/test").unwrap();
    assert_eq!(retrieved.name, "Test Skill");
    assert_eq!(retrieved.downloads, 100);
}

#[test]
fn test_catalog_remove() {
    let mut catalog = populated_catalog();
    assert_eq!(catalog.len(), 4);

    let removed = catalog.remove("org/web-search");
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().name, "Web Search");
    assert_eq!(catalog.len(), 3);
    assert!(catalog.get("org/web-search").is_none());
}

#[test]
fn test_catalog_remove_nonexistent() {
    let mut catalog = SkillCatalog::new();
    assert!(catalog.remove("nonexistent").is_none());
}

#[test]
fn test_catalog_search_by_query() {
    let catalog = populated_catalog();
    let query = CatalogQuery::new().with_query("search");
    let results = catalog.search(&query);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "org/web-search");
}

#[test]
fn test_catalog_search_by_description() {
    let catalog = populated_catalog();
    let query = CatalogQuery::new().with_query("database");
    let results = catalog.search(&query);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "org/db-query");
}

#[test]
fn test_catalog_search_by_tag() {
    let catalog = populated_catalog();
    let query = CatalogQuery::new().with_tag("code");
    let results = catalog.search(&query);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "org/code-runner");
}

#[test]
fn test_catalog_search_by_runtime() {
    let catalog = populated_catalog();
    let query = CatalogQuery::new().with_runtime("python");
    let results = catalog.search(&query);

    assert_eq!(results.len(), 2);
    // Sorted by downloads: web-search (1000) > db-query (800)
    assert_eq!(results[0].id, "org/web-search");
    assert_eq!(results[1].id, "org/db-query");
}

#[test]
fn test_catalog_search_sorted_by_downloads() {
    let catalog = populated_catalog();
    // Empty query returns all, sorted by downloads
    let query = CatalogQuery::new();
    let results = catalog.search(&query);

    assert_eq!(results.len(), 4);
    assert_eq!(results[0].id, "org/code-runner"); // 2000
    assert_eq!(results[1].id, "org/web-search");  // 1000
    assert_eq!(results[2].id, "org/db-query");    // 800
    assert_eq!(results[3].id, "org/file-manager"); // 500
}

#[test]
fn test_catalog_search_with_limit() {
    let catalog = populated_catalog();
    let query = CatalogQuery::new().with_limit(2);
    let results = catalog.search(&query);

    assert_eq!(results.len(), 2);
}

#[test]
fn test_catalog_search_no_results() {
    let catalog = populated_catalog();
    let query = CatalogQuery::new().with_query("nonexistent-skill-xyz");
    let results = catalog.search(&query);

    assert!(results.is_empty());
}

#[test]
fn test_catalog_with_registry() {
    let catalog = SkillCatalog::new()
        .with_registry("https://registry.example.com");

    assert!(catalog.has_registry());
    assert_eq!(catalog.registry_url(), Some("https://registry.example.com"));
}

#[test]
fn test_catalog_list() {
    let catalog = populated_catalog();
    let all = catalog.list();
    assert_eq!(all.len(), 4);
}

#[test]
fn test_catalog_query_combined_filters() {
    let catalog = populated_catalog();
    // Query "code" + runtime "wasm"
    let query = CatalogQuery::new()
        .with_query("code")
        .with_runtime("wasm");
    let results = catalog.search(&query);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "org/code-runner");
}

#[test]
fn test_catalog_query_default_limit() {
    let query = CatalogQuery::new();
    assert_eq!(query.limit, 20);
}
