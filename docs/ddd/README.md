# Domain-Driven Design Models

> **Note**: Current DDD files use a single-file mode.
> **Target**: Migrate to RuView mode (one file per bounded context).

This folder contains Domain-Driven Design (DDD) specifications for the octo-sandbox project.

## Current Structure

| File | Description |
|------|-------------|
| [DDD_DOMAIN_ANALYSIS.md](DDD_DOMAIN_ANALYSIS.md) | Comprehensive domain analysis with bounded contexts, aggregate roots, and ubiquitous language |
| [DDD_CHANGE_LOG.md](DDD_CHANGE_LOG.md) | Change tracking log for architecture modifications |

## Domain Model (from DDD_DOMAIN_ANALYSIS.md)

### Bounded Contexts

| # | Context | Responsibility | Key Modules |
|---|---------|---------------|-------------|
| 1 | Agent Execution | Agent lifecycle, execution loop, routing | `agent/runtime.rs`, `agent/executor.rs`, `agent/loop.rs` |
| 2 | Memory Management | Multi-layer memory, vector search, knowledge graph | `memory/working.rs`, `memory/store.rs` |
| 3 | MCP Integration | MCP server lifecycle, protocol support, tool bridge | `mcp/manager.rs`, `mcp/client.rs` |
| 4 | Tool Execution | Tool registry, execution, risk assessment | `tools/registry.rs`, `tools/executor.rs` |
| 5 | Provider Management | LLM provider chain, failover, load balancing | `providers/chain.rs` |
| 6 | Security Policy | Path validation, autonomy levels, action tracking | `security/policy.rs` |
| 7 | Session Management | Session store, lifecycle, state management | `session/store.rs` |
| 8 | Event System | Event bus, event sourcing, projections | `event/bus.rs`, `event/store.rs` |
| 9 | Hook System | Hook registry, handlers, context propagation | `hooks/registry.rs` |

## How to Read

Each bounded context defines:

- **Ubiquitous Language** — Terms with precise meanings used in both code and conversation
- **Aggregates** — Clusters of objects that enforce business rules
- **Value Objects** — Immutable data with meaning
- **Domain Events** — Things that happened that other contexts may care about
- **Invariants** — Rules that must always be true

## How Agents Use DDD

### Current Mechanism

1. **Manual Reference**: Agents can reference DDD_DOMAIN_ANALYSIS.md when writing code
2. **Change Log**: DDD_CHANGE_LOG.md automatically tracks architecture changes

### Future Mechanism (ADR-012)

According to ADR-012, future implementation will include **ConstraintInjector** to automatically inject relevant DDD constraints into Agent context.

---

## Relationship with ADRs

- [Architecture Decision Records](../adr/README.md) — Why each technical choice was made
- ADRs define boundaries that DDD models must follow
- DDD models define the language that ADRs reference
