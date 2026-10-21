# Codex API Per-Preset Local Proxy Option Design

## Goal

Make `通过 webClx 本机代理转发` a Codex_API preset-level option instead of a global Codex_API switch.

## Behavior

- The Codex_API list no longer exposes a table-level global local-proxy toggle.
- Each Codex_API preset keeps `apply_upstream_proxy_on_switch`.
- Applying a preset uses local proxy routing only when that preset option is saved as enabled.
- GLM, Zhipu, DeepSeek, and MiniMax compatibility rules auto-check the preset-level option while editing when the user has not manually changed it, but the user can cancel it.
- If a recommended preset-level option is unchecked, the editor warns when it is unchecked and warns again before save because the upstream may fail without webClx request adaptation and credential injection.
- Applying a direct preset must not persist `codex_api_proxy_enabled = true` just because a previous preset used local proxy.
- `active_api_proxy_preset_id` remains persisted so `/api/upstream/openai/v1` can route already-started local-proxy processes to the active preset.

## Implementation Boundaries

- Codex_API ownership moves to `StoredApiPreset.apply_upstream_proxy_on_switch`.
- `UpstreamProxySettings.codex_api_proxy_enabled` remains for backward compatibility and existing JSON shape, but Codex_API apply logic must not treat it as authoritative.
- Claude proxy behavior is unchanged.
- Local proxy routes should keep forwarding based on `active_api_proxy_preset_id`, independent of the old global Codex_API bool.

## Verification

- Rust tests cover saved per-preset option behavior, provider recommendation detection, and active summary through local proxy without the global bool.
- Static tests cover removal of the Codex_API global toggle and the preset editor checkbox behavior.
