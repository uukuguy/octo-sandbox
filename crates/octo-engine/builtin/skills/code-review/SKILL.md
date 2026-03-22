---
name: code-review
description: Review code for quality, security, and best practices
version: "1.0"
user-invocable: true
execution-mode: knowledge
always: false
trust-level: installed
triggers:
  - type: keyword
    keyword: review
  - type: keyword
    keyword: code quality
tags:
  - code
  - review
  - quality
---

# Code Review Skill

When reviewing code, follow these guidelines:

## Review Checklist

### Correctness
- Does the code do what it's supposed to do?
- Are edge cases handled?
- Are error conditions properly managed?

### Security
- Input validation at system boundaries
- No hardcoded credentials or secrets
- SQL injection / XSS / command injection prevention
- Proper authentication and authorization checks

### Performance
- No unnecessary allocations or copies
- Efficient algorithms and data structures
- Proper use of async/await where applicable
- No N+1 query patterns

### Maintainability
- Clear variable and function names
- Functions are focused (single responsibility)
- No excessive nesting (max 3 levels)
- Adequate but not excessive comments

### Style
- Consistent formatting
- Follows project conventions
- No dead code or unused imports

## Output Format

Structure your review as:
1. **Summary**: One-line assessment
2. **Critical Issues**: Must-fix problems (security, correctness)
3. **Suggestions**: Improvements (performance, style, maintainability)
4. **Positive Notes**: What's done well
