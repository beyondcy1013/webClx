# Preset Manual Ordering

Stable conclusions:

- Codex_OAuth, Codex_API, and Claude_API preset tables persist manual order by rewriting the existing `Vec<Stored...Preset>` in the requested id order. Reordering must not update `saved_at`, active state, generated config files, or preset secrets.
- Proxy presets must be stored and listed as an ordered `Vec<ProxyPreset>`. A `HashMap` loses user-controlled order and makes table order unstable across reloads.
- Reorder APIs accept a complete id list and reject missing, duplicate, empty, or unknown ids so stale browser state cannot silently drop presets.
- Frontend manual move buttons clear the temporary table sort state before saving, so the persisted manual order is visible immediately.
- The Codex_API table can group by normalized Base URL or by `config_overrides.model`. The browser remembers this display choice locally; it does not alter preset data.
- API move buttons operate inside the currently visible group and persist swaps back to the complete preset vector. The first saved preset for a model is therefore also the preset selected by CLI/model lookup.
