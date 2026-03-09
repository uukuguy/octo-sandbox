use std::collections::{HashMap, HashSet, VecDeque};

use octo_types::skill::SkillDefinition;

/// Error types for dependency resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyError {
    /// A cyclic dependency was detected. Contains the cycle path.
    CyclicDependency(Vec<String>),
    /// A required dependency is missing from the graph.
    MissingDependency { skill: String, missing: String },
}

impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyError::CyclicDependency(path) => {
                write!(f, "Cyclic dependency detected: {}", path.join(" -> "))
            }
            DependencyError::MissingDependency { skill, missing } => {
                write!(
                    f,
                    "Skill '{}' depends on '{}', which is not defined",
                    skill, missing
                )
            }
        }
    }
}

impl std::error::Error for DependencyError {}

/// DAG-based dependency resolver for skills.
///
/// Uses Kahn's algorithm for topological sorting and cycle detection.
pub struct SkillDependencyGraph {
    /// skill_name -> list of dependency names
    edges: HashMap<String, Vec<String>>,
}

impl SkillDependencyGraph {
    /// Build the graph from a set of skill definitions.
    pub fn build(skills: &[SkillDefinition]) -> Self {
        let mut edges = HashMap::new();
        for skill in skills {
            edges.insert(skill.name.clone(), skill.dependencies.clone());
        }
        Self { edges }
    }

    /// Resolve a skill and all its transitive dependencies.
    ///
    /// Returns skills in topological order (dependencies first).
    /// Detects cycles and missing dependencies.
    pub fn resolve(&self, skill_name: &str) -> Result<Vec<String>, DependencyError> {
        // First, collect all reachable nodes via DFS
        let mut visited = HashSet::new();
        let mut subgraph_edges: HashMap<String, Vec<String>> = HashMap::new();
        self.collect_subgraph(skill_name, &mut visited, &mut subgraph_edges)?;

        // Topological sort the subgraph using Kahn's algorithm
        self.topo_sort(&subgraph_edges)
    }

    /// Resolve multiple skills with all their dependencies.
    ///
    /// Deduplicates shared dependencies and returns in topological order.
    pub fn resolve_all(&self, skill_names: &[String]) -> Result<Vec<String>, DependencyError> {
        let mut visited = HashSet::new();
        let mut subgraph_edges: HashMap<String, Vec<String>> = HashMap::new();

        for name in skill_names {
            self.collect_subgraph(name, &mut visited, &mut subgraph_edges)?;
        }

        self.topo_sort(&subgraph_edges)
    }

    /// Check the entire graph for cycles.
    pub fn check_cycles(&self) -> Result<(), DependencyError> {
        let _ = self.topo_sort(&self.edges)?;
        Ok(())
    }

    /// DFS to collect all reachable nodes from a starting skill.
    fn collect_subgraph(
        &self,
        skill_name: &str,
        visited: &mut HashSet<String>,
        subgraph: &mut HashMap<String, Vec<String>>,
    ) -> Result<(), DependencyError> {
        if visited.contains(skill_name) {
            return Ok(());
        }
        visited.insert(skill_name.to_string());

        let deps = match self.edges.get(skill_name) {
            Some(deps) => deps.clone(),
            None => {
                // Node not in graph — this will be caught as MissingDependency
                // by the caller if it's referenced as a dependency.
                // If it's the root node, treat as having no deps.
                subgraph.insert(skill_name.to_string(), vec![]);
                return Ok(());
            }
        };

        for dep in &deps {
            if !self.edges.contains_key(dep) {
                return Err(DependencyError::MissingDependency {
                    skill: skill_name.to_string(),
                    missing: dep.clone(),
                });
            }
            self.collect_subgraph(dep, visited, subgraph)?;
        }

        subgraph.insert(skill_name.to_string(), deps);
        Ok(())
    }

    /// Topological sort using Kahn's algorithm.
    /// Returns an error if cycles are detected.
    fn topo_sort(
        &self,
        edges: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<String>, DependencyError> {
        // edges[A] = [B, C] means A depends on B and C.
        // We want B, C before A in the output (dependencies first).
        // Build reverse adjacency: dep -> [dependents], and in-degree counts
        // how many dependencies each node has.
        let mut in_deg: HashMap<&str, usize> = HashMap::new();
        for node in edges.keys() {
            in_deg.entry(node.as_str()).or_insert(0);
        }
        let mut in_deg: HashMap<&str, usize> = HashMap::new();
        let mut reverse_adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in edges.keys() {
            in_deg.entry(node.as_str()).or_insert(0);
            reverse_adj.entry(node.as_str()).or_default();
        }
        for (node, deps) in edges {
            for dep in deps {
                // node depends on dep => edge dep -> node in reverse graph
                reverse_adj
                    .entry(dep.as_str())
                    .or_default()
                    .push(node.as_str());
                *in_deg.entry(node.as_str()).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<&str> = VecDeque::new();
        for (node, &deg) in &in_deg {
            if deg == 0 {
                queue.push_back(node);
            }
        }

        let mut result = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node.to_string());
            if let Some(dependents) = reverse_adj.get(node) {
                for &dependent in dependents {
                    if let Some(deg) = in_deg.get_mut(dependent) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            }
        }

        if result.len() != edges.len() {
            // Cycle detected — find the cycle for error reporting
            let remaining: Vec<String> = edges
                .keys()
                .filter(|k| !result.contains(k))
                .cloned()
                .collect();
            return Err(DependencyError::CyclicDependency(remaining));
        }

        Ok(result)
    }
}
