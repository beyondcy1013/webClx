// xterm.js answers terminal queries (DA/DSR/CPR/DECRPM/OSC/DCS replies) and
// focus-report mode events through the same onData event as keystrokes.
// Replayed backlog output contains those queries, so the replies are stale:
// they must not be mistaken for user input that cuts the backlog replay short.
(function attachTerminalReportFilter(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalReportFilter = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalReportFilter() {
  const TERMINAL_REPORT_SEQUENCE_PATTERN = new RegExp(
    [
      "\\u001b\\[[IO]",
      "\\u001b\\[[?>]?[0-9;]*[cnR]",
      "\\u001b\\[\\?[0-9;]*\\$y",
      "\\u001b\\[[0-9;]*t",
      "\\u001b\\[\\?[0-9;]*u",
      "\\u001b\\][^\\u0007\\u001b]*(?:\\u0007|\\u001b\\\\)",
      "\\u001bP[^\\u001b]*\\u001b\\\\",
    ].join("|"),
    "g",
  );

  function isTerminalReportSequence(data) {
    return (
      typeof data === "string" &&
      data.length > 0 &&
      data.replace(TERMINAL_REPORT_SEQUENCE_PATTERN, "") === ""
    );
  }

  return {
    TERMINAL_REPORT_SEQUENCE_PATTERN,
    isTerminalReportSequence,
  };
});
