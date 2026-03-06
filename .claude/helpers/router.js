#!/usr/bin/env node
/**
 * Claude Flow Agent Router
 * Routes tasks to optimal agents based on learned patterns
 */

const AGENT_CAPABILITIES = {
  coder: ['code-generation', 'refactoring', 'debugging', 'implementation'],
  tester: ['unit-testing', 'integration-testing', 'coverage', 'test-generation'],
  reviewer: ['code-review', 'security-audit', 'quality-check', 'best-practices'],
  researcher: ['web-search', 'documentation', 'analysis', 'summarization'],
  architect: ['system-design', 'architecture', 'patterns', 'scalability'],
  'backend-dev': ['api', 'database', 'server', 'authentication'],
  'frontend-dev': ['ui', 'react', 'css', 'components'],
  devops: ['ci-cd', 'docker', 'deployment', 'infrastructure'],
};

const TASK_PATTERNS = {
  // Code patterns (EN + CN) — more specific patterns first
  'test|spec|coverage|unit test|integration|测试|单元测试|集成测试|覆盖率|验证': 'tester',
  'implement|create|build|add|write code|实现|创建|构建|添加|编写|开发|写代码': 'coder',
  'review|audit|check|validate|security|审查|审计|检查|校验|安全|代码评审': 'reviewer',
  'research|find|search|documentation|explore|研究|查找|搜索|文档|探索|调研|分析': 'researcher',
  'design|architect|structure|plan|设计|架构|结构|规划|方案': 'architect',

  // Domain patterns (EN + CN)
  'api|endpoint|server|backend|database|接口|服务端|后端|数据库': 'backend-dev',
  'ui|frontend|component|react|css|style|界面|前端|组件|样式|页面': 'frontend-dev',
  'deploy|docker|ci|cd|pipeline|infrastructure|部署|容器|流水线|基础设施|运维': 'devops',

  // Octo-sandbox specific patterns
  'agent|runtime|executor|loop|agent运行|智能体': 'coder',
  'memory|hnsw|vector|embedding|记忆|向量|索引': 'coder',
  'mcp|tool|bridge|工具|桥接': 'coder',
  'hook|event|lifecycle|钩子|事件|生命周期': 'coder',
  'refactor|重构|优化|整理|拆分': 'coder',
  'bug|fix|error|修复|修bug|报错|异常': 'coder',
  'performance|benchmark|perf|性能|基准|优化': 'researcher',
};

// --- Phase 1.5: Learned pattern lookup ---

const path = require('path');
const fs = require('fs');

const SWARM_DIR = path.resolve(__dirname, '../../.swarm');
const MEMORY_DB_PATH = path.join(SWARM_DIR, 'memory.db');
const PATTERNS_JSON_PATH = path.join(SWARM_DIR, 'patterns.json');
const DECAY_RATE = 0.95; // confidence decays ~5% per day since last access

/**
 * Query learned routing patterns from memory.db (SQLite) or patterns.json fallback.
 * Returns { agent, confidence, reason } or null if no match found.
 */
function queryLearnedPatterns(taskLower) {
  try {
    // Try SQLite via better-sqlite3 first
    const patterns = _queryFromSqlite(taskLower);
    if (patterns) return patterns;
  } catch (_) {
    // better-sqlite3 not available or DB error — silent fallback
  }

  try {
    // Fallback: read patterns.json
    return _queryFromJsonFile(taskLower);
  } catch (_) {
    // No patterns file — skip silently
  }

  return null;
}

function _queryFromSqlite(taskLower) {
  let Database;
  try {
    Database = require('better-sqlite3');
  } catch (_) {
    return null; // better-sqlite3 not installed
  }

  if (!fs.existsSync(MEMORY_DB_PATH)) return null;

  const db = Database(MEMORY_DB_PATH, { readonly: true });
  try {
    const rows = db.prepare(
      `SELECT key, content, last_accessed_at, access_count
       FROM memory_entries
       WHERE namespace = 'routing' AND status = 'active'
       ORDER BY access_count DESC`
    ).all();

    return _findBestPattern(rows, taskLower, (row) => {
      const parsed = JSON.parse(row.content);
      return {
        taskPattern: parsed.task || row.key,
        agent: parsed.agent,
        baseConfidence: parsed.confidence || 0.7,
        lastAccessMs: row.last_accessed_at || Date.now(),
      };
    });
  } finally {
    db.close();
  }
}

function _queryFromJsonFile(taskLower) {
  if (!fs.existsSync(PATTERNS_JSON_PATH)) return null;

  const data = JSON.parse(fs.readFileSync(PATTERNS_JSON_PATH, 'utf-8'));
  const entries = Array.isArray(data) ? data : (data.patterns || []);
  if (entries.length === 0) return null;

  return _findBestPattern(entries, taskLower, (entry) => ({
    taskPattern: entry.task || entry.key || '',
    agent: entry.agent,
    baseConfidence: entry.confidence || 0.7,
    lastAccessMs: entry.last_accessed_at || entry.lastAccessedAt || Date.now(),
  }));
}

function _findBestPattern(rows, taskLower, extractFn) {
  const now = Date.now();
  let best = null;

  for (const row of rows) {
    try {
      const { taskPattern, agent, baseConfidence, lastAccessMs } = extractFn(row);
      if (!agent || !taskPattern) continue;

      // Check if the task matches this learned pattern (substring match)
      const patternLower = taskPattern.toLowerCase();
      if (!taskLower.includes(patternLower) && !patternLower.includes(taskLower)) continue;

      // Apply confidence decay based on days since last access
      const daysSince = Math.max(0, (now - lastAccessMs) / (1000 * 60 * 60 * 24));
      const decayed = baseConfidence * Math.pow(DECAY_RATE, daysSince);

      if (decayed > 0.4 && (!best || decayed > best.confidence)) {
        best = { agent, confidence: Math.round(decayed * 1000) / 1000, reason: `Learned pattern: "${taskPattern}"` };
      }
    } catch (_) {
      // Skip malformed entries
    }
  }

  return best;
}

/**
 * Record a routing outcome for future learning.
 * Stores to SQLite if better-sqlite3 is available, otherwise appends to patterns.json.
 */
function recordRouting(task, agent, success) {
  const entry = {
    task,
    agent,
    confidence: success ? 0.75 : 0.3,
    success,
    recorded_at: Date.now(),
    last_accessed_at: Date.now(),
  };

  // Try SQLite first
  try {
    const Database = require('better-sqlite3');
    if (fs.existsSync(MEMORY_DB_PATH)) {
      const db = Database(MEMORY_DB_PATH);
      try {
        const key = `route-${task.substring(0, 80).replace(/[^a-zA-Z0-9\u4e00-\u9fff-]/g, '_')}`;
        const id = `routing-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
        db.prepare(
          `INSERT INTO memory_entries (id, key, namespace, content, type, access_count, last_accessed_at)
           VALUES (?, ?, 'routing', ?, 'pattern', 1, ?)
           ON CONFLICT(namespace, key) DO UPDATE SET
             content = excluded.content,
             access_count = access_count + 1,
             last_accessed_at = excluded.last_accessed_at,
             updated_at = excluded.last_accessed_at`
        ).run(id, key, JSON.stringify(entry), Date.now());
        return;
      } finally {
        db.close();
      }
    }
  } catch (_) {
    // Fall through to JSON
  }

  // Fallback: append to patterns.json
  try {
    if (!fs.existsSync(SWARM_DIR)) fs.mkdirSync(SWARM_DIR, { recursive: true });
    let patterns = [];
    if (fs.existsSync(PATTERNS_JSON_PATH)) {
      const data = JSON.parse(fs.readFileSync(PATTERNS_JSON_PATH, 'utf-8'));
      patterns = Array.isArray(data) ? data : (data.patterns || []);
    }

    // Update existing or append new
    const existing = patterns.findIndex((p) => p.task === task && p.agent === agent);
    if (existing >= 0) {
      patterns[existing].confidence = entry.confidence;
      patterns[existing].last_accessed_at = Date.now();
      patterns[existing].access_count = (patterns[existing].access_count || 0) + 1;
    } else {
      patterns.push({ ...entry, access_count: 1 });
    }

    fs.writeFileSync(PATTERNS_JSON_PATH, JSON.stringify(patterns, null, 2), 'utf-8');
  } catch (_) {
    // Silent failure — routing recording is best-effort
  }
}

// --- End Phase 1.5 helpers ---

// Semantic keyword scoring — gives partial matches higher confidence
const SEMANTIC_KEYWORDS = {
  'coder':       ['代码', '实现', '功能', '模块', '编码', '开发', 'code', 'impl', 'feature', 'module'],
  'tester':      ['测试', '用例', '覆盖', '断言', 'test', 'assert', 'coverage', 'spec'],
  'reviewer':    ['审查', '评审', '质量', '安全', 'review', 'quality', 'audit', 'security'],
  'researcher':  ['分析', '调研', '比较', '探索', '研究', 'analyze', 'research', 'compare', 'explore'],
  'architect':   ['架构', '设计', '方案', '拓扑', 'architecture', 'design', 'topology', 'pattern'],
  'backend-dev': ['API', '接口', '数据库', '服务', 'endpoint', 'database', 'server'],
  'frontend-dev':['界面', '组件', '样式', '页面', 'UI', 'component', 'style', 'page'],
  'devops':      ['部署', '容器', 'CI', 'CD', 'Docker', 'deploy', 'pipeline'],
};

function routeTask(task) {
  const taskLower = task.toLowerCase();

  // Phase 1: Regex pattern matching (high confidence)
  for (const [pattern, agent] of Object.entries(TASK_PATTERNS)) {
    const regex = new RegExp(pattern, 'i');
    if (regex.test(taskLower)) {
      return {
        agent,
        confidence: 0.8,
        reason: `Matched pattern: ${pattern.substring(0, 50)}...`,
      };
    }
  }

  // Phase 1.5: Learned pattern lookup (confidence with decay)
  const learned = queryLearnedPatterns(taskLower);
  if (learned) {
    return learned;
  }

  // Phase 2: Semantic keyword scoring (medium confidence)
  const scores = {};
  for (const [agent, keywords] of Object.entries(SEMANTIC_KEYWORDS)) {
    let score = 0;
    const matched = [];
    for (const kw of keywords) {
      if (taskLower.includes(kw.toLowerCase())) {
        score += 1;
        matched.push(kw);
      }
    }
    if (score > 0) scores[agent] = { score, matched };
  }

  if (Object.keys(scores).length > 0) {
    const sorted = Object.entries(scores).sort((a, b) => b[1].score - a[1].score);
    const [bestAgent, bestData] = sorted[0];
    const confidence = Math.min(0.3 + bestData.score * 0.15, 0.75);
    return {
      agent: bestAgent,
      confidence,
      reason: `Semantic match: ${bestData.matched.join(', ')}`,
      alternatives: sorted.slice(1, 3).map(([a, d]) => ({
        agent: a,
        confidence: Math.min(0.3 + d.score * 0.15, 0.75),
      })),
    };
  }

  // Phase 3: Default fallback
  return {
    agent: 'coder',
    confidence: 0.5,
    reason: 'Default routing - no specific pattern matched',
  };
}

// CLI — only runs when invoked directly, not when require()'d
if (require.main === module) {
  const task = process.argv.slice(2).join(' ');

  if (task) {
    const result = routeTask(task);
    console.log(JSON.stringify(result, null, 2));
  } else {
    console.log('Usage: router.js <task description>');
    console.log('\nAvailable agents:', Object.keys(AGENT_CAPABILITIES).join(', '));
  }
}

module.exports = { routeTask, recordRouting, AGENT_CAPABILITIES, TASK_PATTERNS };
