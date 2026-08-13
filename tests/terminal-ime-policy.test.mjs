import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";

const require = createRequire(import.meta.url);
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalEntryJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalFocusJs = readFileSync(new URL("../static/terminal-focus-selection.js", import.meta.url), "utf8");
const terminalMobileKeysJs = readFileSync(new URL("../static/terminal-mobile-keys.js", import.meta.url), "utf8");
const terminalResumeAgentJs = readFileSync(new URL("../static/terminal-resume-agent.js", import.meta.url), "utf8");
const terminalShellSettingsJs = readFileSync(new URL("../static/terminal-shell-settings.js", import.meta.url), "utf8");
const injectLatestResumeCommandStart = terminalResumeAgentJs.indexOf("function injectLatestResumeCommand()");
const injectLatestResumeCommandEnd = terminalResumeAgentJs.indexOf(
  "function showCopyResumeOverlay",
  injectLatestResumeCommandStart,
);
const injectLatestResumeCommandBody = terminalResumeAgentJs.slice(
  injectLatestResumeCommandStart,
  injectLatestResumeCommandEnd,
);
const focusTerminalIfAllowedStart = terminalFocusJs.indexOf("function focusTerminalIfAllowed()");
const focusTerminalIfAllowedEnd = terminalFocusJs.indexOf(
  "function isTerminalEventTarget",
  focusTerminalIfAllowedStart,
);
const focusTerminalIfAllowedBody = terminalFocusJs.slice(
  focusTerminalIfAllowedStart,
  focusTerminalIfAllowedEnd,
);
const { terminalImeToggleAction } = require("../static/terminal-ime-policy.js");
const {
  TERMINAL_SYSTEM_IME_SUPPRESSION_MS,
  terminalImeDirectFocusAction,
  terminalImeFocusAllowed,
  terminalImeFunctionAction,
} = require("../static/terminal-ime-policy.js");

assert.equal(
  terminalImeToggleAction({ systemImeEnabled: false, helperFocused: false }),
  "focus",
  "disabled IME mode should focus the terminal helper textarea",
);

assert.equal(
  terminalImeToggleAction({ systemImeEnabled: true, helperFocused: false }),
  "focus",
  "enabled IME mode with a blurred helper should refocus instead of toggling off",
);

assert.equal(
  terminalImeToggleAction({ systemImeEnabled: true, helperFocused: true }),
  "disable",
  "enabled IME mode with an already focused helper should toggle off",
);

assert.equal(
  terminalImeFunctionAction({ action: "disable_system_keyboard" }, 1000).suppressedUntil,
  1000 + TERMINAL_SYSTEM_IME_SUPPRESSION_MS,
  "disable system keyboard function should suppress automatic IME focus for one minute",
);

assert.equal(
  terminalImeFocusAllowed({ now: 2000, suppressedUntil: 1000 + TERMINAL_SYSTEM_IME_SUPPRESSION_MS }),
  false,
  "normal terminal focus should not reopen system keyboard during suppression window",
);

assert.equal(
  terminalImeDirectFocusAction({ now: 2000, suppressedUntil: 1000 + TERMINAL_SYSTEM_IME_SUPPRESSION_MS }),
  "blocked",
  "tapping the terminal should not clear the one-minute system keyboard suppression window",
);

assert.equal(
  terminalImeDirectFocusAction({ now: 1000 + TERMINAL_SYSTEM_IME_SUPPRESSION_MS, suppressedUntil: 1000 + TERMINAL_SYSTEM_IME_SUPPRESSION_MS }),
  "focus",
  "tapping the terminal may request direct input after the suppression window expires",
);

assert.equal(
  terminalImeFunctionAction({ action: "show_system_keyboard" }, 2000).suppressedUntil,
  0,
  "explicit show system keyboard command should clear suppression",
);

assert.match(
  terminalShellSettingsJs,
  /function terminalSoftKeyboardVisible\(\) \{[\s\S]*terminalSoftKeyboardAutoVisible\(\) \|\| state\.temporaryDesktopTerminalSoftKeyboardVisible[\s\S]*\}/,
  "IME policy should recognize both automatic and temporarily opened soft keyboards",
);

assert.match(
  terminalFocusJs,
  /function terminalSystemImeSuppressedBySoftKeyboardMode\(\) \{[\s\S]*return !terminalSystemImeEnabled && terminalSoftKeyboardVisible\(\);[\s\S]*\}/,
  "system IME should switch xterm helper textarea to inputmode=none whenever the soft keyboard is active",
);

assert.match(
  terminalFocusJs,
  /function terminalSystemImeFocusSuppressed\(\) \{[\s\S]*terminalSoftKeyboardVisible\(\)[\s\S]*terminalImePolicy\.terminalImeFocusAllowed[\s\S]*terminalSystemImeSuppressedUntil[\s\S]*\}/,
  "the one-minute system-keyboard suppression window should cover every visible soft keyboard",
);

assert.match(
  terminalFocusJs,
  /helper\.setAttribute\([\s\S]*"inputmode",[\s\S]*terminalSystemImeSuppressedBySoftKeyboardMode\(\) \? "none" : "text"[\s\S]*\);/,
  "desktop terminal helper textarea should keep text inputmode so Chinese IME can be selected",
);

assert.match(
  focusTerminalIfAllowedBody,
  /if \(terminalSystemImeFocusSuppressed\(\)\) \{[\s\S]*blurTerminalHelperTextarea\(\);[\s\S]*return;[\s\S]*\}[\s\S]*if \(terminalSystemImeSuppressedBySoftKeyboardMode\(\)\) \{[\s\S]*blurTerminalHelperTextarea\(\);/,
  "terminal focus restoration should blur the helper textarea while soft-keyboard suppression is active",
);

assert.match(
  terminalFocusJs,
  /function focusTerminalAfterSoftKeyboardInput\(\) \{[\s\S]*syncTerminalImePolicy\(\);[\s\S]*\}/,
  "mobile soft-key input should sync the IME policy after taking over input",
);

const softKeyboardFocusHelper = terminalFocusJs.match(/function focusTerminalAfterSoftKeyboardInput\(\) \{([\s\S]*?)\n\}/)?.[1] || "";
assert.doesNotMatch(
  softKeyboardFocusHelper,
  /setTerminalSystemImeEnabled|terminalSystemImeEnabled\s*=|blurTerminalHelperTextarea/,
  "soft-keyboard interaction must preserve the existing system-keyboard state",
);
assert.doesNotMatch(
  softKeyboardFocusHelper,
  /term\.focus|focusTerminalIfAllowed|focusTerminalForDirectInput/,
  "soft-keyboard interaction must never focus the xterm helper textarea",
);

assert.match(
  terminalFocusJs,
  /function focusTerminalForUserInput\(\) \{[\s\S]*if \(terminalSoftKeyboardVisible\(\)\) \{[\s\S]*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*return;[\s\S]*\}[\s\S]*focusTerminalForDirectInput\(\);[\s\S]*\}/,
  "transient controls such as jump-to-top and jump-to-bottom should stay in soft-keyboard mode",
);

assert.match(
  terminalMobileKeysJs,
  /function triggerMobileKey\(button\) \{[\s\S]*queueMobileKeyInput\(button, chunks\);[\s\S]*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*\}/,
  "ordinary mobile soft-key buttons should not reopen the system keyboard after sending input",
);

assert.match(
  terminalMobileKeysJs,
  /function sendTextCommand\(command[\s\S]*sendTerminalInput\(MOBILE_KEY_SEQUENCES\.enter\);[\s\S]*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*\}/,
  "mobile text commands should not reopen the system keyboard after sending input",
);

assert.match(
  injectLatestResumeCommandBody,
  /sendTerminalAutoTypedInput\(command\);[\s\S]*focusTerminalAfterSoftKeyboardInput\(\);/,
  "mobile resume extraction should not reopen the system keyboard after sending input",
);

assert.match(
  terminalMobileKeysJs,
  /function handleTerminalEscapeCommandMenuClick\(event\) \{[\s\S]*sendTerminalInput\(sequence\);[\s\S]*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*\}/,
  "the Esc/^C menu should not reopen the system keyboard after sending input",
);

assert.match(
  terminalMobileKeysJs,
  /function handleTerminalNumberMenuClick\(event\) \{[\s\S]*sendTerminalInput\(digit\);[\s\S]*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*\}/,
  "the number menu should preserve the system keyboard state after sending input",
);

assert.match(
  terminalMobileKeysJs,
  /function prepareMobileKeyControl\(button\) \{[\s\S]*button\.tabIndex = -1;[\s\S]*\}/,
  "mobile soft-key buttons should stay out of the focus chain so they do not invite native keyboard handling",
);

assert.match(
  terminalMobileKeysJs,
  /function suppressMobileKeyNativeEvent\(event\) \{[\s\S]*event\.preventDefault\(\);[\s\S]*event\.stopPropagation\(\);[\s\S]*return button;[\s\S]*\}/,
  "mobile soft-key buttons should suppress native browser keyboard/input events before sending PTY input",
);

assert.match(
  terminalEntryJs,
  /mobileKeysEl\.addEventListener\("keydown", handleMobileKeyKeyboardEvent, true\);[\s\S]*mobileKeysEl\.addEventListener\("keyup", handleMobileKeyKeyboardEvent, true\);[\s\S]*mobileKeysEl\.addEventListener\("beforeinput", handleMobileKeyKeyboardEvent, true\);[\s\S]*mobileKeysEl\.addEventListener\("click", handleMobileKeyClick, true\);/,
  "mobile soft-key container should capture keyboard/input/click events from arrow buttons before the system IME sees them",
);

assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyPointerEnd\(event\) \{[\s\S]*event\.preventDefault\(\);[\s\S]*event\.stopPropagation\(\);[\s\S]*const button = mobileKeyPress\.button;[\s\S]*const hasRepeated = mobileKeyPress\.hasRepeated;/,
  "repeatable mobile arrow keys should suppress native pointer completion even after they have already repeated",
);

assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyPointerDown\(event\) \{\s*preserveSoftKeyboardImeFocus\(event\);\s*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*const button = suppressMobileKeyNativeEvent\(event\);/,
  "every pointer interaction inside the soft keyboard should preserve system IME state, including plain navigation and tool buttons",
);

assert.match(
  terminalMobileKeysJs,
  /function preserveSoftKeyboardImeFocus\(event\)[\s\S]*terminalHelperTextareaFocused\(\)[\s\S]*event\.preventDefault\(\)/,
  "every non-IME control in the soft-keyboard command surfaces should preserve the current helper focus before native focus can change it",
);
assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyFocusIn\(event\)[\s\S]*terminalSystemKeyboardCheckboxEl[\s\S]*softKeyboardImeFocusWasActive[\s\S]*focusTerminalHelperTextareaPreservingIme\(\)[\s\S]*target\.blur\(\)/,
  "the focus safety net should restore an already-open IME but keep a previously closed IME closed",
);
assert.match(
  terminalEntryJs,
  /terminalEscapeCommandMenuEl,[\s\S]*terminalNumberMenuEl,[\s\S]*terminalSlashCommandMenuEl,[\s\S]*terminalFunctionCommandMenuEl,[\s\S]*terminalProjectCommandMenuEl,[\s\S]*terminalToolsMenuEl,[\s\S]*terminalCommandCollectionsMenuEl,[\s\S]*surface\.addEventListener\("pointerdown", preserveSoftKeyboardImeFocus, true\);[\s\S]*surface\.addEventListener\("focusin", handleMobileKeyFocusIn, true\);/,
  "every detached soft-keyboard command menu must use the same IME-state preservation gate",
);
assert.doesNotMatch(
  terminalEntryJs,
  /closeTerminalFunctionCommandMenu\(\);\s*terminalFunctionCommandButtonEl\.focus/,
  "closing the all-purpose command menu with Escape must not move focus away from the terminal helper",
);
assert.match(
  terminalEntryJs,
  /if \(event\.key !== "Escape" \|\| terminalFunctionCommandMenuEl\.hidden\)[\s\S]*closeTerminalFunctionCommandMenu\(\);\s*\}, true\);/,
  "the all-purpose menu should capture Escape before xterm consumes it",
);

assert.match(
  terminalEntryJs,
  /if \(terminalFunctionCommandMenuEl && terminalFunctionCommandButtonEl\) \{[\s\S]*document\.addEventListener\("pointerdown",[\s\S]*closeTerminalFunctionCommandMenu\(\);\s*\}, true\);[\s\S]*window\.addEventListener\("blur", closeTerminalFunctionCommandMenu\);/,
  "the all-purpose menu should close before soft-key propagation is stopped and when the window loses focus",
);

assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyTouchStart\(event\) \{[\s\S]*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*const button = mobileKeyButtonFromEventTarget\(event\.target\);/,
  "every touch interaction inside the soft keyboard should preserve system IME state, including native select controls",
);

assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyClick\(event\) \{\s*const button = suppressMobileKeyNativeEvent\(event\);[\s\S]*if \(!button \|\| window\.PointerEvent\) \{\s*return;\s*\}\s*focusTerminalAfterSoftKeyboardInput\(\);\s*triggerMobileKey\(button\);/,
  "compatibility click handling should not undo an explicit system-keyboard action already dispatched on pointerup",
);

assert.match(
  terminalHtml,
  /terminal-focus-selection\.js\?v=20260803b/,
  "terminal focus helper should use the soft-keyboard system-IME preservation cache version",
);

for (const asset of [
  "terminal-shell-settings.js",
  "terminal-mobile-keys.js",
  "terminal.js",
]) {
  assert.match(
    terminalHtml,
    new RegExp(`${asset.replaceAll(".", "\\.")}\\?v=\\d+[a-z]?`),
    `${asset} should remain cache-versioned`,
  );
}
