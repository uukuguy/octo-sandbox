# Architecture Decision Records

> **Note**: Current ADR files use "multi-section" mode (one file contains multiple ADR sections).
> **Target**: Migrate to RuView mode (one file per ADR).

This folder contains Architecture Decision Records (ADRs) that document every significant technical choice in the octo-sandbox project.

## Why ADRs?

ADRs capture the **context**, **options considered**, **decision made**, and **consequences** for each architectural choice. They serve three purposes:

1. **Institutional memory** — Anyone (human or AI) can read *why* we chose a particular approach, not just see the code.

2. **AI-assisted development** — When an AI agent works on this codebase, ADRs give it the constraints and rationale it needs to make changes that align with the existing architecture.

3. **Review checkpoints** — Each ADR is a reviewable artifact. When a proposed change touches the architecture, the ADR forces the author to articulate tradeoffs *before* writing code.

### ADRs and Domain-Driven Design

The project uses [Domain-Driven Design](../ddd/) (DDD) to organize code into bounded contexts. ADRs and DDD work together:

- **ADRs define boundaries**: Each ADR establishes architectural decisions that bounded contexts must follow.
- **DDD models define the language**: Domain models define terms that ADRs reference precisely.
- **Together they prevent drift**: An AI agent reading ADR-010 knows how the AgentRouter works, because the ADR documents it.

### How ADRs are structured

Each ADR follows a consistent format:

- **Context** — What problem or gap prompted this decision
- **Decision** — What we chose to do and how
- **Consequences** — What improved, what got harder, and what risks remain
- **References** — Related ADRs, papers, and code paths

Statuses: **Proposed** (under discussion), **Accepted** (approved and/or implemented), **Superseded** (replaced by a later ADR), **Completed** (implemented and verified).

---

## ADR Index

### Agent Architecture

| ADR | Title | Status |
|-----|-------|--------|
| ADR-014 | AgentRuntime Modularization | Completed |
| ADR-015 | AgentRouter Routing Decision | Completed |
| ADR-016 | ManifestLoader YAML Declarative Agent | Completed |

### Multi-Agent Orchestration

| ADR | Title | Status |
|-----|-------|--------|
| ADR-006 | Three-Tier Architecture (Engine/Workbench/Platform) | Accepted |
| ADR-007 | Hook Engine Introduction | Proposed |
| ADR-008 | Event Sourcing Introduction | Proposed |
| ADR-009 | HNSW Vector Index Introduction | Proposed |
| ADR-010 | Agent Router Introduction | Proposed |
| ADR-011 | Multi-Agent Topology and Orchestration | Proposed |
| ADR-012 | ADR/DDD Documents as Agent Constraint System | Proposed |

### Security

| ADR | Title | Status |
|-----|-------|--------|
| ADR-001 | PathValidator Security Policy Injection | Accepted |
| ADR-002 | BashTool ExecPolicy Default Enabled | Accepted |
| ADR-003 | API Key Hash Algorithm Upgrade to HMAC-SHA256 | Accepted |
| ADR-004 | Middleware Execution Order Fix (LIFO) | Accepted |
| ADR-005 | AgentRuntime Modular Split | Accepted |
| ADR-006-HMAC | HMAC Secret Force Check (Fail-Fast) | Accepted |
| ADR-007-MCP | MCP call_mcp_tool Lock-Free I/O | Accepted |

### MCP Integration

| ADR | Title | Status |
|-----|-------|--------|
| ADR-013 | MCP Manager Lifecycle Management | Completed |
| ADR-017 | MCP Client Multi-Protocol Support | Completed |
| ADR-018 | MCP Tool Bridge Unified Interface | Completed |

### Memory System

| ADR | Title | Status |
|-----|-------|--------|
| ADR-019 | Four-Layer Memory Architecture | Completed |
| ADR-020 | HNSW Vector Index | Completed |
| ADR-021 | Hybrid Query Engine | Completed |
| ADR-022 | ContextInjector Zone B Dynamic Context | Completed |

### Hooks System

| ADR | Title | Status |
|-----|-------|--------|
| ADR-023 | HookRegistry Global Hook Registration | Completed |
| ADR-024 | HookHandler Event Processing Mechanism | Completed |
| ADR-025 | HookContext Context Propagation | Completed |

### Event Sourcing

| ADR | Title | Status |
|-----|-------|--------|
| ADR-026 | EventBus Event Bus | Completed |
| ADR-027 | EventStore Event Persistence | Completed |
| ADR-028 | ProjectionEngine Projection Engine | Completed |
| ADR-029 | StateReconstructor State Replay | Completed |

---

## How Agents Use ADR/DDD

### Current Mechanism

1. **Manual Reference**: Agents can reference ADR documents when writing code
2. **DDD Change Log**: `docs/ddd/DDD_CHANGE_LOG.md` tracks architecture changes

### Future Mechanism (ADR-012)

According to ADR-012 design, future implementation will include **ConstraintInjector**:

```rust
// context/constraint_injector.rs
pub struct ConstraintInjector {
    adr_index: Vec<AdrEntry>,     // Scanned from docs/adr/
    ddd_index: Vec<DddContext>,   // Scanned from docs/ddd/
}

impl ConstraintInjector {
    /// Find relevant constraints based on task description
    pub fn find_constraints(&self, task: &str) -> Vec<Constraint>;

    /// Format constraints for system prompt
    pub fn format_for_prompt(&self, constraints: &[Constraint]) -> String;
}
```

This will allow agents to automatically search and inject relevant ADR/DDD constraints into system prompts during task execution.

---

## Related Links

- [DDD Domain Models](../ddd/) — Bounded context definitions and ubiquitous language
- [CLAUDE.md](../../CLAUDE.md) — Project instructions for AI agents
- [Design Documents](../design/) — Technical design documents
