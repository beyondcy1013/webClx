# Universal API Proxy Toggle Design

## Goal

Add independent proxy-forwarding switches above the Codex_API preset table and the Claude Code preset table.

When a switch is enabled, applying any preset in that table writes the local webClx proxy endpoint into the tool configuration. webClx then forwards requests to the real upstream from the selected preset. When disabled, applying a preset restores the existing direct-upstream behavior.

## Scope

- Codex_API presets get their own global toggle.
- Claude Code presets get their own global toggle.
- The toggles are persisted in webClx application settings.
- Applying a preset records which upstream preset is active for the corresponding proxy.
- Existing MiniMax, Zhipu, and DeepSeek compatibility behavior must keep working.
- Existing terminal startup fields on Codex_API presets are not changed by this feature.

## Codex_API Behavior

When the Codex_API proxy toggle is disabled, applying an API preset keeps the current behavior:

- Write `auth.json` with the preset API key.
- Write `~/.codex/config.toml` provider `base_url` to the preset `base_url`, except for existing special conversion paths.
- Preserve configured `wire_api` and config override handling.

When the Codex_API proxy toggle is enabled:

- Write `~/.codex/config.toml` provider `base_url` to a local webClx OpenAI-compatible proxy base, such as `http://127.0.0.1:<port>/api/upstream/openai/v1`.
- Keep provider ID as the existing fixed `webclx_api`.
- Prefer proxy-side API key injection so Codex CLI receives a stable placeholder API key rather than the real upstream key.
- Store the selected API preset ID as the current Codex_API upstream.
- webClx forwards OpenAI-compatible requests to the preset `base_url`, injects `Authorization: Bearer <preset.api_key>`, and handles Responses API requests according to the preset `wire_api` and compatibility mode.

For upstreams that cannot handle Responses API directly, the existing Responses-to-Chat conversion path remains available. The previous inferred MiniMax and DeepSeek behavior should be preserved, and the new universal proxy path should be able to use the same conversion helpers when needed.

## Claude Code Behavior

When the Claude proxy toggle is disabled, applying a Claude preset keeps the current behavior:

- Write `~/.claude/settings.json` with the preset token, real `ANTHROPIC_BASE_URL`, model fields, and env overrides.

When the Claude proxy toggle is enabled:

- Write `ANTHROPIC_BASE_URL` to a local webClx Anthropic-compatible proxy base, such as `http://127.0.0.1:<port>/api/upstream/anthropic`.
- Prefer proxy-side token injection so Claude Code receives a stable placeholder token rather than the real upstream token.
- Store the selected Claude preset ID as the current Claude upstream.
- webClx forwards Anthropic-compatible requests to the preset `base_url`, injects the preset token, and preserves request method, path, query string, and relevant Anthropic headers.
- Model fields continue to be written from the selected preset exactly as before.

## UI

Add one compact toggle above each saved preset table:

- Codex_API table: `通过 webClx 本机代理转发`
- Claude Code table: `通过 webClx 本机代理转发`

The toggle state should load with the preset data and save immediately when changed. After changing a toggle, if the table has an active preset, the frontend should re-apply that active preset so the on-disk Codex or Claude configuration immediately matches the new mode.

The table should continue to show the real preset `Base URL`, not only the local proxy URL. Current state text can mention when the active config is routed through webClx.

## Backend Data Model

Persist a small proxy settings object alongside auth preset state:

- `codex_api_proxy_enabled: bool`
- `claude_proxy_enabled: bool`
- `active_api_proxy_preset_id: Option<String>`
- `active_claude_proxy_preset_id: Option<String>`

The active preset IDs are updated when a preset is applied. If an active ID points to a deleted preset, proxy requests return a clear 400/404 style error instead of falling back silently.

## Proxy Endpoints

Add generic local-only proxy routes:

- OpenAI-compatible route: `/api/upstream/openai/v1/*path`
- Anthropic-compatible route: `/api/upstream/anthropic/*path`

Both routes must reject non-loopback clients, matching the existing Codex proxy safety boundary.

OpenAI-compatible forwarding:

- Preserve method, path, query, content type, and body.
- Remove hop-by-hop headers.
- Inject authorization from the active API preset.
- For `/responses` requests whose upstream needs chat conversion, reuse the existing `codex_proxy_core` conversion and response normalization helpers.
- For direct chat or passthrough requests, forward to the selected preset base URL.

Anthropic-compatible forwarding:

- Preserve method, path, query, content type, body, and Anthropic version/beta headers.
- Remove hop-by-hop headers.
- Inject token from the active Claude preset using the header shape expected by the upstream-compatible endpoint.
- Forward to the selected preset base URL.

## Error Handling

- If a proxy endpoint is called while its switch is disabled, return a clear error explaining that the corresponding proxy mode is disabled.
- If no active upstream preset exists, return a clear error asking the user to apply a preset.
- If the stored active preset was deleted, return a clear error and do not guess another preset.
- Upstream non-success status codes should pass through with response body when possible.
- Network errors should include which preset/provider was being contacted.

## Tests

Add Rust tests for:

- Config writers choose local proxy base URL when each switch is enabled.
- Config writers keep direct upstream behavior when disabled.
- Active upstream preset IDs are updated when presets are applied.
- Existing MiniMax/DeepSeek proxy inference behavior still works.
- Claude settings writer can write a local proxy base while keeping model/env behavior.

Add static frontend tests for:

- Codex_API table includes the proxy toggle wiring.
- Claude Code table includes the proxy toggle wiring.
- Toggle changes call the backend and re-apply the active preset.

Add focused proxy tests where practical for:

- OpenAI route rejects non-loopback clients.
- OpenAI route injects the active preset key.
- Claude route injects the active preset token.

## Deployment

Frontend changes must be synced to `/home/bin/webclx/static/` after implementation because the running service reads static files from that deployment directory.

If backend routes or data models change, build the release binary from the Cargo metadata target directory and install it into `/home/bin/webclx/webClx` before restarting `webclx.service`.
