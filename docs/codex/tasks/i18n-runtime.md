# Browser i18n runtime

## Current contract

- Last verified: 2026-08-14.
- The authoritative runtime files are `static/i18n.js`, `static/styles-base.css`, and `static/styles-responsive.css`. The tracked `static` entry is a symlink to `/home/bin/webclx/static`.
- With no saved `webclx:locale`, locale initialization uses `navigator.language` first, then `navigator.languages[0]`, and falls back to `zh-CN` for unsupported languages.
- A manual Chinese or English choice is stored in `webclx:locale` and overrides later system-language detection.
- On application pages, `.browser-topbar` remains a two-column grid at desktop and mobile widths: horizontally scrollable navigation on the left and `#webclx-language-control` on the far upper-right.
- The terminal page's legacy `.browser-topbar-spacer` is replaced by the language control so the selector cannot wrap onto a second row.
- The login page keeps the same control at the upper-right of `.login-card`.

## Verification

```bash
node --test tests/i18n-runtime.test.mjs
node --check static/i18n.js
python3 tests/i18n-live.browser.py
```

The browser test covers workspace, terminal, Agent, and login pages at 1440x900 and 375x812. It verifies system-language initialization, manual persistence, keyboard focus, upper-right geometry, dynamic translation, untranslated controls, overflow, console errors, and failed responses.

Verified live hashes on 2026-08-14:

- `static/i18n.js`: `70d74d634aa7583fb5541cd5302986c9480cbaa118d4b5868c0704d86921a82d`
- `static/styles-base.css`: `3f8de79b68290aa25db324a647a38670295f19b9612fb945276d646aaf97a736`
- `static/styles-responsive.css`: `569638ec16bd7583c4944099bc8e6ebf9106832ad4c0cfc8091529984ffab5ee`

## Invalidation conditions

- The top navigation structure or responsive breakpoint changes.
- The locale storage key, supported locale list, or browser language detection order changes.
- A release archive contains different static hashes; regenerate and audit its static asset manifest rather than mutating an already published version.
