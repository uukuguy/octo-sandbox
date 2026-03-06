#!/usr/bin/env node
/**
 * Professional Code Review Automation
 *
 * Generates structured review prompts for 3 parallel review agents
 * and posts results to GitHub PR via `gh` CLI.
 *
 * Usage:
 *   node code-review.cjs run [--pr <number>] [--base <branch>]
 *   node code-review.cjs post <pr-number> <review-file>
 *   node code-review.cjs config
 *
 * Integrates with hook-handler.cjs post-task hook for automatic triggering.
 */

'use strict';

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const CWD = process.cwd();

// ── Config ──────────────────────────────────────────────────────────────────

function getSettings() {
  const settingsPath = path.join(CWD, '.claude', 'settings.json');
  try {
    if (fs.existsSync(settingsPath)) return JSON.parse(fs.readFileSync(settingsPath, 'utf-8'));
  } catch { /* ignore */ }
  return {};
}

function getReviewConfig() {
  const settings = getSettings();
  const cf = (settings && settings.claudeFlow) || {};
  const review = cf.codeReview || {};
  return {
    enabled: review.enabled !== undefined ? review.enabled : true,
    autoOnPR: review.autoOnPR !== undefined ? review.autoOnPR : true,
    baseBranch: review.baseBranch || 'main',
    owner: review.owner || _detectOwner(),
    repo: review.repo || _detectRepo(),
    reviewers: review.reviewers || [
      { type: 'architect-review', focus: 'architecture' },
      { type: 'security-auditor', focus: 'security' },
      { type: 'code-reviewer', focus: 'quality' },
    ],
    postToGitHub: review.postToGitHub !== undefined ? review.postToGitHub : true,
  };
}

function _detectOwner() {
  try {
    const url = execFileSync('git', ['remote', 'get-url', 'origin'], { encoding: 'utf-8' }).trim();
    const m = url.match(/[:/]([^/]+)\/[^/]+?(?:\.git)?$/);
    return m ? m[1] : 'unknown';
  } catch { return 'unknown'; }
}

function _detectRepo() {
  try {
    const url = execFileSync('git', ['remote', 'get-url', 'origin'], { encoding: 'utf-8' }).trim();
    const m = url.match(/\/([^/]+?)(?:\.git)?$/);
    return m ? m[1] : 'unknown';
  } catch { return 'unknown'; }
}

// ── Review Prompts ──────────────────────────────────────────────────────────

const REVIEW_PROMPTS = {
  'architect-review': (base, files) => `Review the architectural changes between ${base} and HEAD.

Focus on:
1. **DDD Bounded Context integrity** - Do new modules respect bounded context boundaries? Cross-context coupling violations?
2. **Module coupling** - Check lib.rs re-exports. Circular dependencies or tight coupling?
3. **Design pattern correctness** - Event Sourcing, CQRS, Repository patterns properly implemented?
4. **API design** - Are trait extensions backward-compatible? Are public APIs ergonomic?
5. **Scalability** - Will the design scale? Any bottleneck patterns?

Changed files: ${files.join(', ')}

Output a structured review:
- CRITICAL: Must fix before merge
- WARNING: Should fix
- SUGGESTION: Nice to have
- APPROVED: Looks good

Read actual source files. Base review on real code.`,

  'security-auditor': (base, files) => `Perform a security audit of changes between ${base} and HEAD.

Focus on:
1. **Input validation** - Resource URIs, prompt arguments, user inputs properly validated?
2. **SQL injection** - SQLite queries use parameterized statements?
3. **Command injection** - Shell commands properly escaped?
4. **Path traversal** - File paths sanitized?
5. **Error handling** - Errors don't leak internal state or stack traces?
6. **Permissions** - Access control properly enforced?
7. **Secrets** - No hardcoded credentials or API keys?

Changed files: ${files.join(', ')}

Output severity levels: CRITICAL, HIGH, MEDIUM, LOW, INFO.
Include file:line references and remediation steps.`,

  'code-reviewer': (base, files) => `Perform code quality review of changes between ${base} and HEAD.

Focus on:
1. **Error handling** - No unwrap() in production Rust code? Proper Result/Option usage?
2. **Thread safety** - Arc, Mutex, RwLock used correctly? No data races?
3. **Async correctness** - No blocking in async context? Proper .await usage?
4. **Test coverage** - Adequate tests? Edge cases covered?
5. **Code duplication** - DRY violations?
6. **API ergonomics** - Builder patterns, type safety, documentation?
7. **Performance** - Unnecessary allocations? O(n^2) where O(n) suffices?

Changed files: ${files.join(', ')}

Output issues by severity with file:line references and suggested fixes.`,
};

// ── Core Functions ──────────────────────────────────────────────────────────

/**
 * Get changed files between base and HEAD.
 */
function getChangedFiles(base) {
  try {
    const output = execFileSync('git', ['diff', '--name-only', `${base}..HEAD`], { encoding: 'utf-8' });
    return output.trim().split('\n').filter(Boolean);
  } catch {
    return [];
  }
}

/**
 * Generate review agent configurations.
 * Returns array of { type, focus, prompt } for each reviewer.
 */
function generateReviewSpecs(config) {
  const files = getChangedFiles(config.baseBranch);
  if (files.length === 0) {
    return { specs: [], files: [], error: 'No changed files found' };
  }

  const specs = config.reviewers.map(r => {
    const promptFn = REVIEW_PROMPTS[r.type];
    return {
      type: r.type,
      focus: r.focus,
      prompt: promptFn
        ? promptFn(config.baseBranch, files)
        : `Review changes between ${config.baseBranch} and HEAD. Focus: ${r.focus}. Files: ${files.join(', ')}`,
    };
  });

  return { specs, files };
}

/**
 * Post review results to GitHub PR as a comment via `gh` CLI.
 */
function postReviewToGitHub(prNumber, reviewBody) {
  const config = getReviewConfig();
  if (!config.postToGitHub) return { posted: false, reason: 'postToGitHub disabled' };

  try {
    const tmpFile = path.join(CWD, '.claude-flow', 'data', `review-${prNumber}-${Date.now()}.md`);
    const dataDir = path.dirname(tmpFile);
    if (!fs.existsSync(dataDir)) fs.mkdirSync(dataDir, { recursive: true });

    fs.writeFileSync(tmpFile, reviewBody, 'utf-8');

    // Use execFileSync (no shell injection) per project security standards
    const result = execFileSync('gh', [
      'pr', 'comment', String(prNumber),
      '--repo', `${config.owner}/${config.repo}`,
      '--body-file', tmpFile,
    ], { encoding: 'utf-8', timeout: 30000 });

    try { fs.unlinkSync(tmpFile); } catch { /* ignore */ }

    return { posted: true, result: result.trim() };
  } catch (e) {
    return { posted: false, error: e.message };
  }
}

/**
 * Format multiple review results into a single PR comment.
 */
function formatReviewComment(reviews) {
  const timestamp = new Date().toISOString().split('T')[0];
  const lines = [
    `## Code Review Report`,
    `> Auto-generated by Claude (Anthropic AI) via RuFlo Code Review Pipeline | ${timestamp}`,
    '',
  ];

  for (const review of reviews) {
    lines.push(`### ${_focusTitle(review.focus)}`);
    lines.push('');
    if (review.result) {
      lines.push(review.result);
    } else if (review.error) {
      lines.push(`> Review failed: ${review.error}`);
    }
    lines.push('');
    lines.push('---');
    lines.push('');
  }

  lines.push('> 🤖 Generated by **Claude (Anthropic AI)** via Claude Code CLI | Powered by [claude-flow](https://github.com/ruvnet/claude-flow)');
  return lines.join('\n');
}

function _focusTitle(focus) {
  const map = {
    architecture: 'Architecture Review',
    security: 'Security Audit',
    quality: 'Code Quality Review',
  };
  return map[focus] || `${focus} Review`;
}

/**
 * Find open PR number for current branch.
 */
function findOpenPR() {
  const config = getReviewConfig();
  try {
    const branch = execFileSync('git', ['branch', '--show-current'], { encoding: 'utf-8' }).trim();
    const result = execFileSync('gh', [
      'pr', 'list',
      '--repo', `${config.owner}/${config.repo}`,
      '--head', branch,
      '--state', 'open',
      '--json', 'number',
      '--jq', '.[0].number',
    ], { encoding: 'utf-8', timeout: 15000 }).trim();
    return result ? parseInt(result, 10) : null;
  } catch {
    return null;
  }
}

// ── Exports ─────────────────────────────────────────────────────────────────

module.exports = {
  getReviewConfig,
  generateReviewSpecs,
  postReviewToGitHub,
  formatReviewComment,
  findOpenPR,
  REVIEW_PROMPTS,
};

// ── CLI ─────────────────────────────────────────────────────────────────────

if (require.main === module) {
  const cmd = process.argv[2];

  if (cmd === 'run') {
    const config = getReviewConfig();
    if (!config.enabled) {
      console.log('[CODE-REVIEW] Disabled in settings');
      process.exit(0);
    }
    const { specs, files, error } = generateReviewSpecs(config);
    if (error) {
      console.log(`[CODE-REVIEW] ${error}`);
      process.exit(0);
    }
    console.log(JSON.stringify({
      reviewers: specs.length,
      files: files.length,
      pr: findOpenPR(),
      specs: specs.map(s => ({ type: s.type, focus: s.focus })),
    }, null, 2));
  } else if (cmd === 'post') {
    const prNumber = parseInt(process.argv[3], 10);
    const reviewFile = process.argv[4];
    if (!prNumber || !reviewFile) {
      console.error('Usage: code-review.cjs post <pr-number> <review-file>');
      process.exit(1);
    }
    const body = fs.readFileSync(reviewFile, 'utf-8');
    const result = postReviewToGitHub(prNumber, body);
    console.log(JSON.stringify(result, null, 2));
  } else if (cmd === 'config') {
    console.log(JSON.stringify(getReviewConfig(), null, 2));
  } else {
    console.log('Usage: code-review.cjs <run|post|config>');
  }
}
