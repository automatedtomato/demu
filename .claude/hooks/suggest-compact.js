#!/usr/bin/env node
/**
 * suggest-compact.js — PreToolUse hook
 *
 * Counts tool calls in the current session and suggests /compact at
 * logical intervals to prevent context degradation.
 *
 * Counter is stored in a temp file so it resets between sessions.
 * Output is injected as context by Claude Code — Claude sees the message.
 */

const fs = require('fs');
const os = require('os');
const path = require('path');

const COUNTER_FILE = path.join(os.tmpdir(), 'mesh-ant-tool-calls');
const THRESHOLD     = 50;   // First suggestion at this many tool calls
const REMIND_EVERY  = 25;   // Repeat suggestion every N calls after threshold

let count = 0;
try {
  count = parseInt(fs.readFileSync(COUNTER_FILE, 'utf8').trim(), 10) || 0;
} catch (_) {
  // First call of the session — counter file doesn't exist yet
}

count += 1;
fs.writeFileSync(COUNTER_FILE, String(count));

const atThreshold = count === THRESHOLD;
const isReminder  = count > THRESHOLD && (count - THRESHOLD) % REMIND_EVERY === 0;

if (atThreshold || isReminder) {
  const lines = [
    `[compact-suggest] ${count} tool calls this session.`,
    `Context may be degrading. Consider /compact before continuing.`,
    ``,
    `Good times to compact:`,
    `  - After completing a milestone (e.g. M1.1 done, moving to M1.2)`,
    `  - After a planning phase, before implementation`,
    `  - Before switching to an unrelated task`,
    `  - After debugging a dead-end (clear the noise)`,
    ``,
    `Current task context: see tasks/todo.md`,
    `To compact with a hint: /compact Focus on [what comes next]`,
  ];
  process.stdout.write(lines.join('\n') + '\n');
}
