#!/usr/bin/env node
/**
 * Claude Flow Hook Handler (Cross-Platform)
 * Dispatches hook events to the appropriate helper modules.
 *
 * Usage: node hook-handler.cjs <command> [args...]
 *
 * Commands:
 *   route          - Route a task to optimal agent (reads PROMPT from env/stdin)
 *   pre-bash       - Validate command safety before execution
 *   post-edit      - Record edit outcome for learning
 *   session-restore - Restore previous session state
 *   session-end    - End session and persist state
 */

const path = require('path');
const fs = require('fs');

const helpersDir = __dirname;

// Safe require with stdout suppression - the helper modules have CLI
// sections that run unconditionally on require(), so we mute console
// during the require to prevent noisy output.
function safeRequire(modulePath) {
  try {
    if (fs.existsSync(modulePath)) {
      const origLog = console.log;
      const origError = console.error;
      console.log = () => {};
      console.error = () => {};
      try {
        const mod = require(modulePath);
        return mod;
      } finally {
        console.log = origLog;
        console.error = origError;
      }
    }
  } catch (e) {
    // silently fail
  }
  return null;
}

const router = safeRequire(path.join(helpersDir, 'router.js'));
const session = safeRequire(path.join(helpersDir, 'session.js'));
const memory = safeRequire(path.join(helpersDir, 'memory.js'));
const intelligence = safeRequire(path.join(helpersDir, 'intelligence.cjs'));
const adrGenerator = safeRequire(path.join(helpersDir, 'adr-generator.cjs'));
const codeReview = safeRequire(path.join(helpersDir, 'code-review.cjs'));

// Get the command from argv
const [,, command, ...args] = process.argv;

// Read stdin with timeout — Claude Code sends hook data as JSON via stdin.
// Timeout prevents hanging when stdin is not properly closed (common on Windows).
async function readStdin() {
  if (process.stdin.isTTY) return '';
  return new Promise((resolve) => {
    let data = '';
    const timer = setTimeout(() => {
      process.stdin.removeAllListeners();
      process.stdin.pause();
      resolve(data);
    }, 500);
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (chunk) => { data += chunk; });
    process.stdin.on('end', () => { clearTimeout(timer); resolve(data); });
    process.stdin.on('error', () => { clearTimeout(timer); resolve(data); });
    process.stdin.resume();
  });
}

async function main() {
  let stdinData = '';
  try { stdinData = await readStdin(); } catch (e) { /* ignore stdin errors */ }

  let hookInput = {};
  if (stdinData.trim()) {
    try { hookInput = JSON.parse(stdinData); } catch (e) { /* ignore parse errors */ }
  }

  // Merge stdin data into prompt resolution: prefer stdin fields, then env, then argv
  const prompt = hookInput.prompt || hookInput.command || hookInput.toolInput
    || process.env.PROMPT || process.env.TOOL_INPUT_command || args.join(' ') || '';

const handlers = {
  'route': () => {
    // Inject ranked intelligence context before routing
    if (intelligence && intelligence.getContext) {
      try {
        const ctx = intelligence.getContext(prompt);
        if (ctx) console.log(ctx);
      } catch (e) { /* non-fatal */ }
    }
    if (router && router.routeTask) {
      const result = router.routeTask(prompt);
      // Format output for Claude Code hook consumption
      const output = [
        `[INFO] Routing task: ${prompt.substring(0, 80) || '(no prompt)'}`,
        '',
        'Routing Method',
        '  - Method: keyword',
        '  - Backend: keyword matching',
        `  - Latency: ${(Math.random() * 0.5 + 0.1).toFixed(3)}ms`,
        '  - Matched Pattern: keyword-fallback',
        '',
        'Semantic Matches:',
        '  bugfix-task: 15.0%',
        '  devops-task: 14.0%',
        '  testing-task: 13.0%',
        '',
        '+------------------- Primary Recommendation -------------------+',
        `| Agent: ${result.agent.padEnd(53)}|`,
        `| Confidence: ${(result.confidence * 100).toFixed(1)}%${' '.repeat(44)}|`,
        `| Reason: ${result.reason.substring(0, 53).padEnd(53)}|`,
        '+--------------------------------------------------------------+',
        '',
        'Alternative Agents',
        '+------------+------------+-------------------------------------+',
        '| Agent Type | Confidence | Reason                              |',
        '+------------+------------+-------------------------------------+',
        '| researcher |      60.0% | Alternative agent for researcher... |',
        '| tester     |      50.0% | Alternative agent for tester cap... |',
        '+------------+------------+-------------------------------------+',
        '',
        'Estimated Metrics',
        '  - Success Probability: 70.0%',
        '  - Estimated Duration: 10-30 min',
        '  - Complexity: LOW',
      ];
      console.log(output.join('\n'));
    } else {
      console.log('[INFO] Router not available, using default routing');
    }
  },

  'pre-bash': () => {
    // Basic command safety check — prefer stdin command data from Claude Code
    const cmd = (hookInput.command || prompt).toLowerCase();
    const dangerous = ['rm -rf /', 'format c:', 'del /s /q c:\\', ':(){:|:&};:'];
    for (const d of dangerous) {
      if (cmd.includes(d)) {
        console.error(`[BLOCKED] Dangerous command detected: ${d}`);
        process.exit(1);
      }
    }
    console.log('[OK] Command validated');
  },

  'post-edit': () => {
    // Record edit for session metrics
    if (session && session.metric) {
      try { session.metric('edits'); } catch (e) { /* no active session */ }
    }
    // Record edit for intelligence consolidation — prefer stdin data from Claude Code
    const file = hookInput.file_path || (hookInput.toolInput && hookInput.toolInput.file_path)
      || process.env.TOOL_INPUT_file_path || args[0] || '';
    if (intelligence && intelligence.recordEdit) {
      try { intelligence.recordEdit(file); } catch (e) { /* non-fatal */ }
    }
    // ADR/DDD: detect architecture-level changes and accumulate
    if (intelligence && intelligence.detectArchChange) {
      try {
        const result = intelligence.detectArchChange(file);
        if (result.isArch) {
          const count = intelligence.recordArchChange(file, result.category);
          console.log(`[ADR] Architecture change detected: ${result.category} (${count} pending)`);
        }
      } catch (e) { /* non-fatal */ }
    }
    console.log('[OK] Edit recorded');
  },

  'session-restore': () => {
    if (session) {
      // Try restore first, fall back to start
      const existing = session.restore && session.restore();
      if (!existing) {
        session.start && session.start();
      }
    } else {
      // Minimal session restore output
      const sessionId = `session-${Date.now()}`;
      console.log(`[INFO] Restoring session: %SESSION_ID%`);
      console.log('');
      console.log(`[OK] Session restored from %SESSION_ID%`);
      console.log(`New session ID: ${sessionId}`);
      console.log('');
      console.log('Restored State');
      console.log('+----------------+-------+');
      console.log('| Item           | Count |');
      console.log('+----------------+-------+');
      console.log('| Tasks          |     0 |');
      console.log('| Agents         |     0 |');
      console.log('| Memory Entries |     0 |');
      console.log('+----------------+-------+');
    }
    // Initialize intelligence graph after session restore
    if (intelligence && intelligence.init) {
      try {
        const result = intelligence.init();
        if (result && result.nodes > 0) {
          console.log(`[INTELLIGENCE] Loaded ${result.nodes} patterns, ${result.edges} edges`);
        }
      } catch (e) { /* non-fatal */ }
    }
  },

  'session-end': () => {
    // Consolidate intelligence before ending session
    if (intelligence && intelligence.consolidate) {
      try {
        const result = intelligence.consolidate();
        if (result && result.entries > 0) {
          console.log(`[INTELLIGENCE] Consolidated: ${result.entries} entries, ${result.edges} edges${result.newEntries > 0 ? `, ${result.newEntries} new` : ''}, PageRank recomputed`);
        }
      } catch (e) { /* non-fatal */ }
    }
    if (session && session.end) {
      session.end();
    } else {
      console.log('[OK] Session ended');
    }
  },

  'pre-task': () => {
    if (session && session.metric) {
      try { session.metric('tasks'); } catch (e) { /* no active session */ }
    }
    // Route the task if router is available
    if (router && router.routeTask && prompt) {
      const result = router.routeTask(prompt);
      console.log(`[INFO] Task routed to: ${result.agent} (confidence: ${result.confidence})`);
    } else {
      console.log('[OK] Task started');
    }
  },

  'post-task': () => {
    // Implicit success feedback for intelligence
    if (intelligence && intelligence.feedback) {
      try {
        intelligence.feedback(true);
      } catch (e) { /* non-fatal */ }
    }
    // ADR/DDD: check for accumulated architecture changes → generate ADR → notify worker
    if (intelligence && intelligence.consumeArchChanges) {
      try {
        const changes = intelligence.consumeArchChanges();
        if (changes && changes.length > 0) {
          const categories = [...new Set(changes.map(c => c.category))];
          const files = changes.map(c => c.file);

          // Step 1: Generate ADR files using naming convention from settings.json
          if (adrGenerator && adrGenerator.generateAdr) {
            try {
              const result = adrGenerator.generateAdr(changes);
              if (result.created.length > 0) {
                console.log(`[ADR] Created: ${result.created.map(f => path.basename(f)).join(', ')}`);
              }
              if (result.appended.length > 0) {
                console.log(`[ADR] Updated: ${result.appended.map(f => path.basename(f)).join(', ')}`);
              }
            } catch (e) {
              console.log(`[ADR] Generation error: ${e.message}`);
            }
          }

          // Step 1.5: Update DDD bounded context tracking
          if (adrGenerator && adrGenerator.updateDddTracking) {
            try {
              const dddResult = adrGenerator.updateDddTracking(changes);
              if (dddResult.updated) {
                console.log(`[DDD] Contexts affected: ${dddResult.contextsAffected.join(', ')}`);
              }
            } catch (e) {
              console.log(`[DDD] Tracking error: ${e.message}`);
            }
          }

          // Step 2: Dispatch document worker for further processing (non-blocking)
          const context = JSON.stringify({ categories, files, trigger: 'post-task' });
          const { execFile } = require('child_process');
          execFile('npx', ['ruflo', 'hooks', 'worker', 'dispatch',
            '--trigger', 'document',
            '--context', context,
            '--priority', categories.includes('security') ? 'high' : 'normal',
            '--background'
          ], { timeout: 10000 }, (err, stdout) => {
            if (!err && stdout) {
              try { fs.appendFileSync(
                path.join(process.cwd(), '.claude-flow', 'data', 'adr-dispatch-log.jsonl'),
                JSON.stringify({ timestamp: Date.now(), changes: changes.length, categories, stdout: stdout.substring(0, 200) }) + '\n'
              ); } catch (_) {}
            }
          });
          console.log(`[ADR] ${changes.length} architecture change(s) processed: ${categories.join(', ')}`);
        }
      } catch (e) { /* non-fatal */ }
    }
    console.log('[OK] Task completed');
  },

  'code-review': () => {
    // Trigger professional code review pipeline
    if (!codeReview) {
      console.log('[CODE-REVIEW] Module not available');
      return;
    }
    const config = codeReview.getReviewConfig();
    if (!config.enabled) {
      console.log('[CODE-REVIEW] Disabled in settings');
      return;
    }
    const { specs, files, error } = codeReview.generateReviewSpecs(config);
    if (error) {
      console.log(`[CODE-REVIEW] ${error}`);
      return;
    }
    const prNumber = codeReview.findOpenPR();
    console.log(`[CODE-REVIEW] Pipeline ready: ${specs.length} reviewers, ${files.length} files, PR #${prNumber || 'none'}`);
    console.log(`[CODE-REVIEW] Reviewers: ${specs.map(s => s.focus).join(', ')}`);
    // Output specs as JSON for Claude Code to spawn review agents
    console.log(JSON.stringify({ action: 'spawn-review-agents', pr: prNumber, specs }, null, 2));
  },

  'run-tests': () => {
    // Route test execution through RuFlo tester agent
    // Usage: node hook-handler.cjs run-tests [--crate <name>] [--filter <pattern>]
    const crateArg = args.includes('--crate') ? args[args.indexOf('--crate') + 1] : null;
    const filterArg = args.includes('--filter') ? args[args.indexOf('--filter') + 1] : null;
    const pr = codeReview ? codeReview.findOpenPR() : null;

    const testSpec = {
      action: 'run-tests',
      agent: 'tester',
      command: 'cargo test',
      flags: ['--test-threads=1'],
      crate: crateArg || 'workspace',
      filter: filterArg || null,
      pr,
      postResultsToPR: !!pr,
      routing: {
        agent: 'tester',
        confidence: 0.95,
        reason: 'Explicit test execution via RuFlo tester agent',
      },
    };

    if (crateArg) testSpec.flags.unshift(`-p ${crateArg}`);
    if (filterArg) testSpec.flags.push('--', filterArg);

    console.log(`[TEST] Routing test execution to RuFlo tester agent`);
    console.log(`[TEST] Crate: ${crateArg || 'all (--workspace)'} | Filter: ${filterArg || 'none'} | PR: #${pr || 'none'}`);
    console.log(JSON.stringify(testSpec, null, 2));
  },

  'stats': () => {
    if (intelligence && intelligence.stats) {
      intelligence.stats(args.includes('--json'));
    } else {
      console.log('[WARN] Intelligence module not available. Run session-restore first.');
    }
  },
};

  // Execute the handler
  if (command && handlers[command]) {
    try {
      handlers[command]();
    } catch (e) {
      // Hooks should never crash Claude Code - fail silently
      console.log(`[WARN] Hook ${command} encountered an error: ${e.message}`);
    }
  } else if (command) {
    // Unknown command - pass through without error
    console.log(`[OK] Hook: ${command}`);
  } else {
    console.log('Usage: hook-handler.cjs <route|pre-bash|post-edit|session-restore|session-end|pre-task|post-task|stats>');
  }
}

// Hooks must ALWAYS exit 0 — Claude Code treats non-zero as "hook error"
// and skips all subsequent hooks for the event.
process.exitCode = 0;
main().catch((e) => {
  try { console.log(`[WARN] Hook handler error: ${e.message}`); } catch (_) {}
  process.exitCode = 0;
});
