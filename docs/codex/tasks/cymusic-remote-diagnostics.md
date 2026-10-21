# cyMusic Remote Diagnostics

## Purpose

Persist bounded Android playback diagnostics on webClx so cyMusic media failures can be analyzed
without device-side ADB access.

## Contract

- `POST /api/cymusic/diagnostics` accepts an unauthenticated LAN JSON batch with `device_id`,
  `session_id`, `app_version`, and `log`.
- IDs contain only ASCII letters, digits, `-`, or `_`; the request body and log field are bounded.
- Logs are stored below `<app_dir>/cymusic-diagnostics/<device_id>/`, with at most 20 files retained
  per device.
- `GET /api/cymusic/diagnostics?device_id=...` remains behind normal webClx login middleware.
- The Android producer must redact credentials and full NAS URLs before upload.

## Verification

After deployment, POST a synthetic probe to the loopback endpoint and verify the returned receipt
and the corresponding file below `/home/bin/webclx/cymusic-diagnostics/`. Query access must still
require authentication.
