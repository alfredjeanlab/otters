# Runbook Module

TOML-based declarative workflow definitions with template rendering and validation.

## Overview

Runbooks define primitives for orchestrating agentic workflows. This module provides a three-stage pipeline:

```mermaid
graph LR
    TOML["TOML file"] -->|parser| Raw["RawRunbook"]
    Raw -->|validator| Valid["ValidatedRunbook"]
    Valid -->|loader| Runbook["Runbook"]
```

Each stage has distinct responsibilities:
- **parser**: Syntactic parsing (TOML string → RawRunbook)
- **validator**: Semantic validation (RawRunbook → ValidatedRunbook)
- **loader**: Runtime type conversion (ValidatedRunbook → Runbook)

## Landing Checklist

- [ ] Templates render correctly with all input types
- [ ] Validation catches invalid references
- [ ] Cross-runbook references resolve correctly
- [ ] Error messages include file/line information
- [ ] All primitives have corresponding test coverage
