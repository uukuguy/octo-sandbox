use std::path::PathBuf;

use octo_engine::skills::{DependencyError, SkillDependencyGraph};
use octo_types::skill::{SkillDefinition, SkillSourceType, TrustLevel};

fn make_skill(name: &str, deps: Vec<&str>) -> SkillDefinition {
    SkillDefinition {
        name: name.to_string(),
        description: format!("Test skill {}", name),
        version: None,
        user_invocable: false,
        allowed_tools: None,
        denied_tools: None,
        body: String::new(),
        base_dir: PathBuf::from("/test"),
        source_path: PathBuf::from("/test/SKILL.md"),
        body_loaded: false,
        model: None,
        context_fork: false,
        always: false,
        trust_level: TrustLevel::default(),
        triggers: vec![],
        dependencies: deps.into_iter().map(String::from).collect(),
        tags: vec![],
        execution_mode: Default::default(),
        source_type: SkillSourceType::default(),
        max_rounds: 0,
    }
}

#[test]
fn test_no_dependencies() {
    let skills = vec![make_skill("A", vec![])];
    let graph = SkillDependencyGraph::build(&skills);
    let result = graph.resolve("A").unwrap();
    assert_eq!(result, vec!["A".to_string()]);
}

#[test]
fn test_simple_dependency() {
    let skills = vec![make_skill("A", vec!["B"]), make_skill("B", vec![])];
    let graph = SkillDependencyGraph::build(&skills);
    let result = graph.resolve("A").unwrap();
    assert_eq!(result, vec!["B".to_string(), "A".to_string()]);
}

#[test]
fn test_transitive_dependency() {
    let skills = vec![
        make_skill("A", vec!["B"]),
        make_skill("B", vec!["C"]),
        make_skill("C", vec![]),
    ];
    let graph = SkillDependencyGraph::build(&skills);
    let result = graph.resolve("A").unwrap();
    // C must come before B, B must come before A
    let pos_c = result.iter().position(|x| x == "C").unwrap();
    let pos_b = result.iter().position(|x| x == "B").unwrap();
    let pos_a = result.iter().position(|x| x == "A").unwrap();
    assert!(pos_c < pos_b);
    assert!(pos_b < pos_a);
}

#[test]
fn test_cycle_detection() {
    let skills = vec![make_skill("A", vec!["B"]), make_skill("B", vec!["A"])];
    let graph = SkillDependencyGraph::build(&skills);
    let result = graph.resolve("A");
    assert!(matches!(result, Err(DependencyError::CyclicDependency(_))));
}

#[test]
fn test_missing_dependency() {
    let skills = vec![make_skill("A", vec!["X"])];
    let graph = SkillDependencyGraph::build(&skills);
    let result = graph.resolve("A");
    assert!(matches!(
        result,
        Err(DependencyError::MissingDependency {
            skill: _,
            missing: _,
        })
    ));
    if let Err(DependencyError::MissingDependency { skill, missing }) = result {
        assert_eq!(skill, "A");
        assert_eq!(missing, "X");
    }
}

#[test]
fn test_resolve_all() {
    let skills = vec![
        make_skill("A", vec!["C"]),
        make_skill("B", vec!["C"]),
        make_skill("C", vec![]),
    ];
    let graph = SkillDependencyGraph::build(&skills);
    let result = graph
        .resolve_all(&["A".to_string(), "B".to_string()])
        .unwrap();
    // C should appear only once and before both A and B
    assert_eq!(result.iter().filter(|x| *x == "C").count(), 1);
    let pos_c = result.iter().position(|x| x == "C").unwrap();
    let pos_a = result.iter().position(|x| x == "A").unwrap();
    let pos_b = result.iter().position(|x| x == "B").unwrap();
    assert!(pos_c < pos_a);
    assert!(pos_c < pos_b);
}

#[test]
fn test_diamond_dependency() {
    // A -> B, A -> C, B -> D, C -> D
    let skills = vec![
        make_skill("A", vec!["B", "C"]),
        make_skill("B", vec!["D"]),
        make_skill("C", vec!["D"]),
        make_skill("D", vec![]),
    ];
    let graph = SkillDependencyGraph::build(&skills);
    let result = graph.resolve("A").unwrap();
    assert_eq!(result.len(), 4);
    // D must come before B and C; B and C must come before A
    let pos_d = result.iter().position(|x| x == "D").unwrap();
    let pos_b = result.iter().position(|x| x == "B").unwrap();
    let pos_c = result.iter().position(|x| x == "C").unwrap();
    let pos_a = result.iter().position(|x| x == "A").unwrap();
    assert!(pos_d < pos_b);
    assert!(pos_d < pos_c);
    assert!(pos_b < pos_a);
    assert!(pos_c < pos_a);
}
