#!/usr/bin/env node
/**
 * ADR/DDD Auto-Generator
 *
 * Reads architecture changes accumulated by intelligence.cjs and generates
 * ADR documents following the naming convention in settings.json.
 *
 * Config (from .claude/settings.json → claudeFlow.adr):
 *   directory:      "/docs/adr"
 *   filePattern:    "ADR_*.md"
 *   sectionPattern: "^## ADR-\\d+"
 *   naming:         "ADR_{TOPIC}.md"
 *   template:       "madr"
 *
 * Called by hook-handler.cjs post-task when architecture changes are detected.
 */

'use strict';

const fs = require('fs');
const path = require('path');

const CWD = process.cwd();

// ── Config ──────────────────────────────────────────────────────────────────

function getSettings() {
  const settingsPath = path.join(CWD, '.claude', 'settings.json');
  try {
    if (fs.existsSync(settingsPath)) return JSON.parse(fs.readFileSync(settingsPath, 'utf-8'));
  } catch { /* ignore */ }
  return null;
}

function getAdrConfig() {
  const settings = getSettings();
  const cf = (settings && settings.claudeFlow) || {};
  return {
    directory: ((cf.adr && cf.adr.directory) || '/docs/adr').replace(/^\//, ''),
    naming: (cf.adr && cf.adr.naming) || 'ADR_{TOPIC}.md',
    template: (cf.adr && cf.adr.template) || 'madr',
    sectionPattern: (cf.adr && cf.adr.sectionPattern) || '^## ADR-\\d+',
  };
}

function getDddConfig() {
  const settings = getSettings();
  const cf = (settings && settings.claudeFlow) || {};
  return {
    directory: ((cf.ddd && cf.ddd.directory) || '/docs/ddd').replace(/^\//, ''),
  };
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function ensureDir(dir) {
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
}

function getNextAdrNumber(adrDir, sectionPattern) {
  let maxNum = 0;
  try {
    if (!fs.existsSync(adrDir)) return 1;
    const regex = new RegExp(sectionPattern, 'gm');
    const files = fs.readdirSync(adrDir).filter(f => f.endsWith('.md'));
    for (const file of files) {
      const content = fs.readFileSync(path.join(adrDir, file), 'utf-8');
      const matches = content.matchAll(/ADR-(\d+)/g);
      for (const m of matches) {
        const num = parseInt(m[1], 10);
        if (num > maxNum) maxNum = num;
      }
    }
  } catch { /* ignore */ }
  return maxNum + 1;
}

// Map category to human-readable topic name
const CATEGORY_TOPICS = {
  'security': 'SECURITY',
  'agent-architecture': 'AGENT_ARCHITECTURE',
  'mcp-integration': 'MCP_INTEGRATION',
  'memory-architecture': 'MEMORY_ARCHITECTURE',
  'provider-chain': 'PROVIDER_CHAIN',
  'dependency-change': 'DEPENDENCY',
  'api-change': 'API_CHANGE',
  'structural-change': 'STRUCTURAL',
};

// ── ADR Generation ──────────────────────────────────────────────────────────

/**
 * Generate ADR file for a set of architecture changes.
 *
 * @param {Array<{file: string, category: string, timestamp: number}>} changes
 * @returns {{ created: string[], appended: string[], skipped: string[] }}
 */
function generateAdr(changes) {
  const config = getAdrConfig();
  const adrDir = path.join(CWD, config.directory);
  ensureDir(adrDir);

  const result = { created: [], appended: [], skipped: [] };

  // Group changes by category
  const byCategory = {};
  for (const change of changes) {
    const cat = change.category || 'structural-change';
    if (!byCategory[cat]) byCategory[cat] = [];
    byCategory[cat].push(change);
  }

  for (const [category, catChanges] of Object.entries(byCategory)) {
    const topic = CATEGORY_TOPICS[category] || category.toUpperCase().replace(/-/g, '_');
    const fileName = config.naming.replace('{TOPIC}', topic);
    const filePath = path.join(adrDir, fileName);

    // Check if ADR file for this topic already exists
    if (fs.existsSync(filePath)) {
      // Append new section to existing file
      const existing = fs.readFileSync(filePath, 'utf-8');
      const nextNum = getNextAdrNumber(adrDir, config.sectionPattern);

      const newSection = generateAdrSection(nextNum, category, catChanges);

      // Append after last ADR section
      fs.appendFileSync(filePath, '\n---\n\n' + newSection);
      result.appended.push(filePath);
    } else {
      // Create new ADR file
      const nextNum = getNextAdrNumber(adrDir, config.sectionPattern);
      const content = generateAdrFile(nextNum, topic, category, catChanges);
      fs.writeFileSync(filePath, content, 'utf-8');
      result.created.push(filePath);
    }
  }

  return result;
}

function generateAdrFile(startNum, topic, category, changes) {
  const date = new Date().toISOString().split('T')[0];
  const title = topic.replace(/_/g, ' ');
  const section = generateAdrSection(startNum, category, changes);

  return `# ADR：${title} 架构决策记录

**项目**：octo-sandbox
**日期**：${date}
**状态**：待审阅
**自动生成**：由 RuFlo post-task hook 触发

---

${section}
`;
}

function generateAdrSection(num, category, changes) {
  const date = new Date().toISOString().split('T')[0];
  const padNum = String(num).padStart(3, '0');
  const files = changes.map(c => c.file);
  const title = getCategoryTitle(category);

  return `## ADR-${padNum}：${title}

### 状态

**待审阅** — ${date}（自动生成）

### 上下文

以下文件发生了架构级变更，需要记录决策：

${files.map(f => '- `' + f + '`').join('\n')}

### 变更类别

- **类别**：${category}
- **影响范围**：${files.length} 个文件
- **检测时间**：${date}

### 决策

> **待补充**：请审阅上述变更并补充架构决策的具体内容、替代方案和理由。

### 后果

#### 正面
- （待补充）

#### 负面
- （待补充）

### 涉及文件

${files.map(f => '| `' + f + '` | 变更 |').join('\n')}
`;
}

function getCategoryTitle(category) {
  const titles = {
    'security': '安全策略变更',
    'agent-architecture': 'Agent 架构变更',
    'mcp-integration': 'MCP 集成变更',
    'memory-architecture': '记忆架构变更',
    'provider-chain': 'Provider 链变更',
    'dependency-change': '依赖变更',
    'api-change': 'API 接口变更',
    'structural-change': '结构性变更',
  };
  return titles[category] || category;
}

// ── DDD Auto-Update ─────────────────────────────────────────────────────────

// Map architecture changes to DDD bounded contexts
const CONTEXT_MAPPING = {
  'agent-architecture': 'Agent 执行上下文',
  'security':           '安全策略上下文',
  'mcp-integration':    'MCP 集成上下文',
  'memory-architecture': '记忆管理上下文',
  'provider-chain':     'Provider 上下文',
  'api-change':         'API 接口上下文',
  'structural-change':  '通用结构',
};

// File path patterns → bounded context
const PATH_TO_CONTEXT = [
  { pattern: /agent\//,    context: 'Agent 执行上下文' },
  { pattern: /security\//,  context: '安全策略上下文' },
  { pattern: /mcp\//,       context: 'MCP 集成上下文' },
  { pattern: /memory\//,    context: '记忆管理上下文' },
  { pattern: /tools\//,     context: '工具执行上下文' },
  { pattern: /providers?\//,context: 'Provider 上下文' },
  { pattern: /auth\//,      context: '认证授权上下文' },
  { pattern: /event\//,     context: '可观测性上下文' },
  { pattern: /session\//,   context: '会话管理上下文' },
  { pattern: /hooks?\//,    context: '编排上下文' },
  { pattern: /orchestrat/,  context: '编排上下文' },
  { pattern: /sandbox\//,   context: '沙箱执行上下文' },
];

/**
 * Update DDD tracking log when architecture changes affect bounded contexts.
 *
 * @param {Array<{file: string, category: string, timestamp: number}>} changes
 * @returns {{ updated: boolean, contextsAffected: string[], logFile: string }}
 */
function updateDddTracking(changes) {
  const config = getDddConfig();
  const dddDir = path.join(CWD, config.directory);
  ensureDir(dddDir);

  const logFile = path.join(dddDir, 'DDD_CHANGE_LOG.md');
  const result = { updated: false, contextsAffected: [], logFile };

  // Identify affected bounded contexts
  const contextsSet = new Set();
  for (const change of changes) {
    // By category
    const catCtx = CONTEXT_MAPPING[change.category];
    if (catCtx) contextsSet.add(catCtx);

    // By file path
    for (const mapping of PATH_TO_CONTEXT) {
      if (mapping.pattern.test(change.file)) {
        contextsSet.add(mapping.context);
      }
    }
  }

  if (contextsSet.size === 0) return result;

  result.contextsAffected = [...contextsSet];
  result.updated = true;

  const date = new Date().toISOString().split('T')[0];
  const time = new Date().toISOString().split('T')[1].substring(0, 5);
  const files = changes.map(c => c.file);

  const entry = `
### ${date} ${time} — 限界上下文变更

**受影响的限界上下文**：${result.contextsAffected.join('、')}

**变更文件**：
${files.map(f => '- `' + f + '`').join('\n')}

**变更类别**：${[...new Set(changes.map(c => getCategoryTitle(c.category)))].join('、')}

> 请检查 \`DDD_DOMAIN_ANALYSIS.md\` 中对应限界上下文的类型定义和聚合根是否需要更新。

---
`;

  // Append or create log file
  if (fs.existsSync(logFile)) {
    const existing = fs.readFileSync(logFile, 'utf-8');
    fs.writeFileSync(logFile, existing + entry, 'utf-8');
  } else {
    const header = `# DDD 变更追踪日志

> 由 RuFlo post-task hook 自动生成。
> 记录每次架构变更对限界上下文的影响，提醒更新 DDD 领域模型。

---
`;
    fs.writeFileSync(logFile, header + entry, 'utf-8');
  }

  return result;
}

// ── Exports ─────────────────────────────────────────────────────────────────

module.exports = { generateAdr, updateDddTracking, getAdrConfig, getDddConfig };

// ── CLI ─────────────────────────────────────────────────────────────────────

if (require.main === module) {
  const cmd = process.argv[2];

  if (cmd === 'generate') {
    let changes;
    try {
      const input = process.argv[3] || '[]';
      changes = JSON.parse(input);
    } catch {
      console.error('Usage: adr-generator.cjs generate \'[{"file":"...","category":"..."}]\'');
      process.exit(1);
    }
    const adrResult = generateAdr(changes);
    const dddResult = updateDddTracking(changes);
    console.log(JSON.stringify({ adr: adrResult, ddd: dddResult }, null, 2));
  } else if (cmd === 'config') {
    console.log('ADR:', JSON.stringify(getAdrConfig(), null, 2));
    console.log('DDD:', JSON.stringify(getDddConfig(), null, 2));
  } else {
    console.log('Usage: adr-generator.cjs <generate|config>');
  }
}
