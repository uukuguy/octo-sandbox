---
name: readme-writer
description: Generate comprehensive README documentation for projects
version: "1.0"
user-invocable: true
execution-mode: knowledge
always: false
trust-level: installed
triggers:
  - type: keyword
    keyword: readme
  - type: keyword
    keyword: documentation
tags:
  - docs
  - readme
  - documentation
---

# README Writer Skill

When generating README documentation, follow this structure:

## README Template

### Title and Badges
- Project name as H1
- Build status, version, license badges where applicable

### Description
- One-paragraph summary of what the project does
- Key value proposition

### Quick Start
- Installation steps (numbered list)
- Minimum viable example to get started
- Expected output

### Features
- Bullet list of key features
- Brief description for each

### Usage
- Common use cases with code examples
- Configuration options
- CLI commands (if applicable)

### API Reference (if applicable)
- Key functions/methods with signatures
- Parameter descriptions
- Return types and examples

### Development
- How to set up the development environment
- How to run tests
- How to contribute

### License
- License type and link

## Guidelines

- Be concise - users want to get started quickly
- Use code blocks with language identifiers
- Include real, working examples
- Keep the README under 500 lines
- Link to detailed docs for complex topics instead of including everything
