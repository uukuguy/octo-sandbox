---
name: code-debugger
description: Debug issues systematically using step-by-step analysis
version: "1.0"
user-invocable: true
execution-mode: knowledge
always: false
trust-level: installed
triggers:
  - type: keyword
    keyword: debug
  - type: keyword
    keyword: bug
  - type: keyword
    keyword: error
  - type: keyword
    keyword: fix
tags:
  - debug
  - troubleshoot
  - fix
---

# Code Debugger Skill

When debugging issues, follow this systematic approach:

## Debugging Process

### 1. Understand the Problem
- What is the expected behavior?
- What is the actual behavior?
- When did it start happening?
- Can it be reproduced consistently?

### 2. Gather Evidence
- Read error messages carefully - they usually point to the cause
- Check relevant log output
- Identify the specific file(s) and line(s) involved
- Look at recent changes that might have introduced the issue

### 3. Form Hypotheses
- Based on the evidence, list possible causes (most likely first)
- Consider: typos, logic errors, missing imports, wrong types, race conditions

### 4. Test Hypotheses
- Start with the most likely cause
- Make minimal, targeted changes
- Verify each fix before moving on

### 5. Verify the Fix
- Confirm the original issue is resolved
- Check for regressions
- Run existing tests if available

## Common Patterns

- **TypeError/Missing field**: Check struct definitions, function signatures
- **Panic/Unwrap**: Look for Option/Result handling issues
- **Compile error**: Read the full error message, check types and imports
- **Runtime error**: Add logging to trace execution flow
- **Test failure**: Compare expected vs actual output carefully

## Guidelines

- Never guess - always verify with evidence
- Fix the root cause, not the symptom
- One fix at a time to isolate changes
- Document what was wrong and why the fix works
