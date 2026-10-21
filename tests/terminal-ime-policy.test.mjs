import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";

const require = createRequire(import.meta.url);
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalEntryJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalFocusJs = readFileSync(new URL("../static/terminal-focus-selection.js", import.meta.url), "utf8");
const terminalDialogsJs = readFileSync(new URL("../static/terminal-dialogs.js", import.meta.url), "utf8");
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
  /function terminalNativeKeyboardVisible\(\) \{[\s\S]*WebClxAndroid\?\.isSystemKeyboardVisible[\s\S]*isSystemKeyboardVisible\(\) === true[\s\S]*visualViewport/,
  "Android native IME visibility must take priority over browser viewport guesses",
);
assert.match(
  terminalFocusJs,
  /function guardTerminalActionEditableFocus\(event\)[\s\S]*terminalActionNativeKeyboardWasVisible = terminalNativeKeyboardVisible\(\)[\s\S]*terminalActionEditableFocusBlocked = !terminalActionNativeKeyboardWasVisible;[\s\S]*if \(terminalActionEditableFocusBlocked\) \{[\s\S]*blurTerminalHelperTextarea\(\);/,
  "ordinary actions must synchronously blur stale xterm focus when native Android reports the keyboard closed",
);
assert.match(
  terminalFocusJs,
  /function focusTerminalFromTerminalTap\(\) \{[\s\S]*terminalActionKeyboardPolicyActive = false;[\s\S]*terminalActionEditableFocusBlocked = false;[\s\S]*terminalActionNativeKeyboardWasVisible = terminalNativeKeyboardVisible\(\);[\s\S]*focusTerminalForDirectInput\(\);/,
  "a direct tap in the terminal input surface must clear the ordinary-action gate before focusing xterm",
);

assert.match(
  terminalFocusJs,
  /function guardTerminalActionEditableFocus\(event\)[\s\S]*terminalEditableFocusTarget\(target\)[\s\S]*terminalActionKeyboardPolicyActive = false[\s\S]*terminalExplicitSystemKeyboardGestureTarget\(target\)[\s\S]*terminalActionControlTarget\(target\)[\s\S]*terminalActionNativeKeyboardWasVisible = terminalNativeKeyboardVisible\(\)[\s\S]*terminalActionKeyboardPolicyActive = true[\s\S]*terminalActionEditableFocusBlocked = !terminalActionNativeKeyboardWasVisible[\s\S]*event\.preventDefault\(\)/,
  "all action controls must enter the global editable-focus gate except direct editable gestures and the authorized system-keyboard control",
);
assert.match(
  terminalFocusJs,
  /function terminalActionControlTarget\(target\)[\s\S]*target\.closest\(\"select\"\)[\s\S]*return null;/,
  "native select controls inside labels must keep their pointer default action",
);
assert.match(
  terminalFocusJs,
  /function installTerminalProgrammaticFocusGate\(\)[\s\S]*HTMLElement\.prototype\.focus[\s\S]*terminalActionKeyboardPolicyActive[\s\S]*terminalActionEditableFocusBlocked && terminalEditableFocusTarget\(this\)[\s\S]*terminalActionNativeKeyboardWasVisible[\s\S]*terminalEditableFocusTarget\(activeElement\)[\s\S]*this !== activeElement[\s\S]*nativeFocus\.apply/,
  "ordinary actions must reject programmatic focus before an editable control can summon the keyboard",
);
assert.match(
  terminalFocusJs,
  /function installTerminalDialogFocusGate\(\)[\s\S]*\["show", "showModal"\][\s\S]*terminalActionKeyboardPolicyActive[\s\S]*terminalDialogEditableControls\(this\)[\s\S]*setAttribute\("inert", ""\)[\s\S]*nativeShow\.apply/,
  "dialogs opened while the keyboard is closed must suppress native editable autofocus",
);
assert.match(
  terminalFocusJs,
  /function resetTerminalImeFocusContext\(\{ desktopInput = false \} = \{\}\) \{[\s\S]*terminalActionPreservesVisibleNativeKeyboard\(\)[\s\S]*syncTerminalImePolicy\(\);[\s\S]*return;/,
  "ordinary dialog actions must not blur an already-visible native keyboard",
);
assert.match(
  terminalFocusJs,
  /function rejectTerminalActionEditableFocus\(event\)[\s\S]*!terminalActionEditableFocusBlocked[\s\S]*terminalEditableFocusTarget\(target\)[\s\S]*target\.blur\(\)[\s\S]*syncTerminalImePolicy\(\)/,
  "the global gate must synchronously reject editable focus leaked by any button action",
);
assert.match(
  terminalFocusJs,
  /function terminalActionFocusGateExempt\(target\)[\s\S]*terminalClipboardHelper[\s\S]*function rejectTerminalActionEditableFocus\(event\)[\s\S]*terminalActionFocusGateExempt\(target\)[\s\S]*return;[\s\S]*terminalActionEditableFocusBlocked/,
  "the global focusin gate must preserve the hidden textarea used by clipboard fallback",
);
assert.match(
  terminalEntryJs,
  /installTerminalProgrammaticFocusGate\(\);[\s\S]*installTerminalDialogFocusGate\(\);[\s\S]*document\.addEventListener\("pointerdown", guardTerminalActionEditableFocus, true\);[\s\S]*document\.addEventListener\("click", guardTerminalActionEditableFocus, true\);[\s\S]*document\.addEventListener\("focusin", rejectTerminalActionEditableFocus, true\);/,
  "the editable-focus gate must run in capture phase before every terminal action handler",
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
  /function sendTerminalEscapeCtrlCInput\(sequenceKey\) \{[\s\S]*sendTerminalInput\(sequence\);[\s\S]*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*\}/,
  "the Esc/^C key should not reopen the system keyboard after sending input",
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
  /mobileKeysEl\.addEventListener\("keydown", handleMobileKeyKeyboardEvent, true\);[\s\S]*mobileKeysEl\.addEventListener\("keyup", handleMobileKeyKeyboardEvent, true\);[\s\S]*mobileKeysEl\.addEventListener\("beforeinput", handleMobileKeyKeyboardEvent, true\);[\s\S]*mobileKeysEl\.addEventListener\("click", handleMobileKeyClick, true\);[\s\S]*mobileKeysEl\.addEventListener\("pointerup", restoreSystemImeAfterSoftKeyboardControl, true\);[\s\S]*mobileKeysEl\.addEventListener\("pointercancel", restoreSystemImeAfterSoftKeyboardControl, true\);[\s\S]*mobileKeysEl\.addEventListener\("click", restoreSystemImeAfterSoftKeyboardControl, true\);/,
  "mobile soft-key container should restore the pre-gesture system-IME state after pointer or click dispatch",
);

assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyPointerEnd\(event\) \{[\s\S]*event\.preventDefault\(\);[\s\S]*event\.stopPropagation\(\);[\s\S]*const button = mobileKeyPress\.button;[\s\S]*const hasRepeated = mobileKeyPress\.hasRepeated;/,
  "repeatable mobile arrow keys should suppress native pointer completion even after they have already repeated",
);

assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyPointerDown\(event\) \{\s*suppressSystemImeForSoftKeyboardControl\(event\);\s*focusTerminalAfterSoftKeyboardInput\(\);[\s\S]*const button = suppressMobileKeyNativeEvent\(event\);/,
  "every pointer interaction inside the soft keyboard should suppress system IME unless it is the explicit system-keyboard control",
);

assert.match(
  terminalMobileKeysJs,
  /function suppressSystemImeForSoftKeyboardControl\(event\)[\s\S]*explicitSystemImeControl\(control\)[\s\S]*event\.preventDefault\(\)/,
  "every non-IME control in the soft-keyboard command surfaces should block native focus before it can summon the system keyboard",
);
assert.match(
  terminalMobileKeysJs,
  /function restoreSystemImeAfterSoftKeyboardControl\(event\)[\s\S]*softKeyboardImeFocusSnapshots\.get\(control\)[\s\S]*snapshot\.sequence !== softKeyboardImeGestureSequence[\s\S]*if \(!snapshot\.focused\) \{[\s\S]*blurTerminalHelperTextarea\(\)[\s\S]*\} else \{[\s\S]*syncTerminalImePolicy\(\)[\s\S]*window\.requestAnimationFrame\(restore\)[\s\S]*\[80, 180, 320\]\.forEach/,
  "ordinary controls may preserve existing focus but must never refocus the terminal helper",
);
assert.doesNotMatch(
  terminalMobileKeysJs,
  /focusTerminalHelperTextareaPreservingIme/,
  "no ordinary soft-key path may contain a helper refocus escape hatch",
);
assert.doesNotMatch(
  terminalMobileKeysJs,
  /softKeyboardImeDesiredFocus|softKeyboardImeDesiredFocusUntil/,
  "a new control gesture must sample current helper focus instead of inheriting stale focus from an earlier gesture",
);
assert.match(
  terminalMobileKeysJs,
  /const isMainSoftKey = Boolean\(mobileKeysEl\?\.contains\(control\)\);[\s\S]*!isMainSoftKey && now < softKeyboardMenuImeFocusUntil[\s\S]*terminalHelperTextareaFocused\(\)[\s\S]*if \(isMainSoftKey\) \{[\s\S]*softKeyboardMenuImeFocus = helperWasFocused/,
  "main soft keys such as Home must sample current focus while nested menu items inherit only their menu chain state",
);
assert.match(
  terminalEntryJs,
  /scrollTerminalBottomButton[\s\S]*preserveSystemImeStateForControl\(scrollTerminalBottomButton\)/,
  "the jump-to-bottom control must preserve a manually dismissed system keyboard",
);
assert.match(
  terminalMobileKeysJs,
  /const label = target\.closest\("label"\);[\s\S]*label\?\.querySelector\('input\[type="checkbox"\]'\)[\s\S]*labeledCheckbox \|\| target\.closest\("button, input\[type=checkbox\]"\)/,
  "checkbox labels such as 触摸复制 must participate in the same IME-state preservation gate",
);
assert.match(
  terminalMobileKeysJs,
  /function explicitSystemImeControl\(control\) \{\s*return control === terminalSystemKeyboardCheckboxEl;\s*\}/,
  "only the 系统键盘 checkbox in 全能 may bypass system-IME suppression",
);
assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyFocusIn\(event\)[\s\S]*terminalSystemKeyboardCheckboxEl[\s\S]*target\.blur\(\)[\s\S]*softKeyboardImeFocusSnapshots\.get\(control\)[\s\S]*if \(!snapshot\?\.focused\) \{[\s\S]*blurTerminalHelperTextarea\(\)[\s\S]*\} else \{[\s\S]*syncTerminalImePolicy\(\)/,
  "the focus safety net must not refocus the helper after native focus leakage",
);
assert.match(
  terminalEntryJs,
  /terminalNumberMenuEl,[\s\S]*terminalSlashCommandMenuEl,[\s\S]*terminalFunctionCommandMenuEl,[\s\S]*terminalProjectCommandMenuEl,[\s\S]*terminalToolsMenuEl,[\s\S]*terminalToolMenuEl,[\s\S]*terminalCommandCollectionsMenuEl,[\s\S]*surface\.addEventListener\("pointerdown", suppressSystemImeForSoftKeyboardControl, true\);[\s\S]*surface\.addEventListener\("pointerup", restoreSystemImeAfterSoftKeyboardControl, true\);[\s\S]*surface\.addEventListener\("pointercancel", restoreSystemImeAfterSoftKeyboardControl, true\);[\s\S]*surface\.addEventListener\("click", restoreSystemImeAfterSoftKeyboardControl, true\);[\s\S]*surface\.addEventListener\("focusin", handleMobileKeyFocusIn, true\);/,
  "every detached soft-keyboard command menu must use the same system-IME suppression gate",
);
assert.match(
  terminalMobileKeysJs,
  /function handleMobileKeyPointerEnd\(event\)[\s\S]*triggerMobileKey\(button\);\s*restoreSystemImeAfterSoftKeyboardControl\(\{ target: button \}\);/,
  "the main soft-key dispatcher must restore system-IME focus after every non-system-keyboard action",
);
assert.match(
  terminalMobileKeysJs,
  /function runTerminalKeyboardCommand\(action, \{ source = null \} = \{\}\)[\s\S]*if \(imeAction\.kind === "show"\) \{\s*if \(source !== terminalSystemKeyboardCheckboxEl\) \{[\s\S]*blurTerminalHelperTextarea\(\);[\s\S]*return false;/,
  "show_system_keyboard must reject every caller except the 全能 system-keyboard checkbox",
);
assert.match(
  terminalMobileKeysJs,
  /if \(button\.dataset\.action === "toggle_ime"\) \{\s*blurTerminalHelperTextarea\(\);\s*syncTerminalKeyboardCheckboxes\(\);\s*return;\s*\}/,
  "legacy toggle_ime buttons must never focus or reopen the system keyboard",
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
  /terminal-focus-selection\.js\?v=[0-9a-z]+/,
  "terminal focus helper should carry a cache version param",
);

assert.doesNotMatch(
  terminalHtml,
  /autofocus/,
  "terminal dialogs must not contain native autofocus that can alter keyboard state",
);
assert.match(
  terminalFocusJs,
  /function terminalActionFocusGateExempt\(target\)[\s\S]*terminalClipboardHelper[\s\S]*session-rename-input[\s\S]*function installTerminalProgrammaticFocusGate\(\)[\s\S]*terminalActionFocusGateExempt\(this\)[\s\S]*function rejectTerminalActionEditableFocus\(event\)[\s\S]*terminalActionFocusGateExempt\(target\)/,
  "rename input focus should pass both terminal keyboard focus gates",
);
assert.doesNotMatch(
  terminalDialogsJs.slice(
    terminalDialogsJs.indexOf("function openTerminalPasteDialog"),
    terminalDialogsJs.indexOf("function closeTerminalPasteDialog"),
  ),
  /terminalPaste(?:Text|ScheduleDelay)El\??\.focus/,
  "opening paste or schedule controls must not actively focus an editable field",
);
assert.match(
  terminalHtml,
  /terminal-dialogs\.js\?v=[0-9a-z]+/,
  "terminal dialogs should use the no-autofocus cache version",
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
