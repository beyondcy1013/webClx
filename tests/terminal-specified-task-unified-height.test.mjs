import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("styles-terminal.css specifies unified height and anti-compression layout for terminal specified task dialog", () => {
  const styles = readFileSync(new URL("../static/styles-terminal.css", import.meta.url), "utf8");

  // 1. Unified height on #terminal-specified-task-dialog
  assert.match(
    styles,
    /#terminal-specified-task-dialog\s*\{[\s\S]*?height:\s*min\(640px,\s*calc\(100dvh\s*-\s*32px\)\);[\s\S]*?max-height:\s*calc\(100dvh\s*-\s*32px\);/,
    "must specify unified height and max-height for #terminal-specified-task-dialog",
  );

  // 2. Flex column container on #terminal-specified-task-form
  assert.match(
    styles,
    /#terminal-specified-task-form\s*\{[\s\S]*?display:\s*flex;[\s\S]*?flex-direction:\s*column;[\s\S]*?height:\s*100%;[\s\S]*?max-height:\s*100%;[\s\S]*?box-sizing:\s*border-box;/,
    "must specify flex column layout filling 100% height on #terminal-specified-task-form",
  );

  // 3. Direct children prevent compression (flex-shrink: 0)
  assert.match(
    styles,
    /#terminal-specified-task-form\s*>\s*\*[\s\S]*?flex-shrink:\s*0;/,
    "upper controls must not compress when space changes",
  );

  // 4. Fixed options prevent compression
  assert.match(
    styles,
    /#terminal-specified-task-form\s+\.terminal-specified-task-fixed-options\s*\{[\s\S]*?flex-shrink:\s*0;/,
    "fixed options container must not compress",
  );
  assert.match(
    styles,
    /#terminal-specified-task-form\s+\.terminal-specified-task-fixed-options\s*>\s*\*[\s\S]*?flex-shrink:\s*0;/,
    "session action controls inside fixed options must not compress",
  );

  // 5. Prompt field flexibly absorbs remaining vertical space
  assert.match(
    styles,
    /#terminal-specified-task-form\s+\.terminal-specified-task-prompt-field\s*\{[\s\S]*?grid-template-rows:\s*minmax\(0,\s*1fr\);[\s\S]*?flex:\s*1\s+1\s+auto;/,
    "prompt field must flex to absorb height differences between actions and agent types",
  );

  // 6. Textarea expands within prompt field
  assert.match(
    styles,
    /#terminal-specified-task-form\s+\.terminal-specified-task-text\s*\{[\s\S]*?height:\s*100%;[\s\S]*?resize:\s*vertical;/,
    "textarea must fill prompt field height",
  );

  // 7. Dialog actions anchored at bottom
  assert.match(
    styles,
    /#terminal-specified-task-form\s+\.dialog-actions\s*\{[\s\S]*?margin-top:\s*auto;[\s\S]*?flex-shrink:\s*0;/,
    "dialog actions must anchor at bottom",
  );

  // 8. Hidden elements remain hidden regardless of flex rules
  assert.match(
    styles,
    /#terminal-specified-task-form\s+\[hidden\]\s*\{[\s\S]*?display:\s*none\s*!important;/,
    "hidden elements must be strictly hidden",
  );

  // 9. Scrollbar gutter is stable to ensure control widths never fluctuate
  assert.match(
    styles,
    /#terminal-specified-task-form\s*\{[\s\S]*?scrollbar-gutter:\s*stable;/,
    "must specify scrollbar-gutter: stable to prevent width jumping between tabs",
  );

  // 10. Group option uses 4-track grid to stay on one line
  assert.match(
    styles,
    /\.terminal-specified-task-group-option\s*\{[\s\S]*?grid-template-columns:\s*auto\s+auto\s+minmax\(130px,\s*1fr\)\s+minmax\(100px,\s*0\.7fr\);/,
    "group options must use 4 tracks to stay on single line",
  );
});

test("terminal-specified-task.js retains workspace directory field across all session action sub-tabs", () => {
  const code = readFileSync(new URL("../static/terminal-specified-task.js", import.meta.url), "utf8");
  // Ensure workspaceField.hidden is not conditioned on sessionAction === "new"
  assert.doesNotMatch(
    code,
    /workspaceSelectable\s*=\s*[^;]*sessionAction\s*===?\s*["']new["']/,
    "workspace directory must not be restricted only to new session action",
  );
  assert.match(
    code,
    /const\s+workspaceSelectable\s*=\s*!terminalSpecifiedTaskId;[\s\S]*?if\s*\(\s*dom\.workspaceField\s*\)\s*\{\s*dom\.workspaceField\.hidden\s*=\s*false;/,
    "workspace directory field must remain visible in every session action sub-tab",
  );
});

