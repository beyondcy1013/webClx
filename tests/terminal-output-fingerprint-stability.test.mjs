import assert from "node:assert/strict";
import fs from "node:fs";

const tmuxSource = fs.readFileSync(
  new URL("../src/terminal/tmux.rs", import.meta.url),
  "utf8",
);

assert.match(
  tmuxSource,
  /fn capture_tmux_joined_pane_snapshot_from\([\s\S]*capture_tmux_pane_snapshot_from_options\([\s\S]*true,?\s*\)[\s\S]*fn capture_tmux_pane_snapshot_from_options\([\s\S]*join_wrapped_lines: bool,[\s\S]*if join_wrapped_lines \{\s*command\.arg\("-J"\);\s*\}/,
  "tmux snapshots used for output fingerprints must support joining soft-wrapped rows",
);

assert.match(
  tmuxSource,
  /fn capture_tmux_recent_pane_snapshot\(session_id: &str\)[\s\S]{0,240}capture_tmux_joined_pane_snapshot_from\(session_id, "-200", false\)/,
  "viewed-output acknowledgement must fingerprint plain text with wrapped rows joined",
);

assert.match(
  tmuxSource,
  /fn capture_tmux_activity_pane_snapshot\([\s\S]{0,320}capture_tmux_joined_pane_snapshot_from\([\s\S]{0,220}false\)/,
  "activity scans must use the same plain, wrap-independent snapshot representation",
);

console.log("terminal output fingerprint stability tests passed");
