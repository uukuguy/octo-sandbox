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

// CLI
const task = process.argv.slice(2).join(' ');

if (task) {
  const result = routeTask(task);
  console.log(JSON.stringify(result, null, 2));
} else {
  console.log('Usage: router.js <task description>');
  console.log('\nAvailable agents:', Object.keys(AGENT_CAPABILITIES).join(', '));
}

module.exports = { routeTask, AGENT_CAPABILITIES, TASK_PATTERNS };
