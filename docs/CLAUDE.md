# Documentation Organization

Guidelines for organizing documentation in the otters project.

## Directory Structure

```
docs/
├── 01-concepts/        # Core concepts and terminology
├── 02-integrations/    # External system integrations
├── 03-interface/       # CLI and API documentation
├── 04-architecture/    # Technical architecture and diagrams
├── 10-example-runbooks/# Example runbook configurations
└── ZZ-psuedocode/      # Design pseudocode and sketches
```

## Documentation Types

### CLAUDE.md Files

Located in source directories (`crates/core/src/*/CLAUDE.md`), these provide:

- **Module overview**: Brief description of purpose
- **Invariants**: Key rules that must always hold
- **Landing checklist**: Pre-commit verification steps
- **Quick reference**: Tables, code examples

Keep CLAUDE.md files concise. Use mermaid diagrams for state machines. Link to detailed docs/ files for complex diagrams.

### docs/ Files

For detailed documentation:

- **Concepts**: Background knowledge needed to understand the system
- **Architecture**: Detailed diagrams, state machines, data flows
- **Integrations**: How external systems connect

## Naming Conventions

- Use numbered prefixes for ordering: `01-`, `02-`, etc.
- Use kebab-case for file names: `strategy-state-machine.md`
- Use descriptive names that indicate content type

## Landing Checklist

Before adding documentation:
- [ ] Place in appropriate directory (CLAUDE.md vs docs/)
- [ ] Use correct diagram type (mermaid vs ASCII)
- [ ] Add cross-references where appropriate
- [ ] Follow naming conventions
