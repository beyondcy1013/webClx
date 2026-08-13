#!/usr/bin/env python3
"""Send a tagged, optionally verified message to a webClx terminal."""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


DEFAULT_URL = "http://127.0.0.1:11111"
SUPPORTED_AGENTS = ("codex", "claude", "deepseek")
LOCAL_TOKEN_HEADER = "X-WebClx-Local-Token"


class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


HTTP_OPENER = urllib.request.build_opener(NoRedirectHandler())


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default="", help="Terminal name or stable session id.")
    parser.add_argument("--agent", choices=SUPPORTED_AGENTS, default="")
    parser.add_argument("--path", default="", help="Optional workspace path used to narrow discovery.")
    parser.add_argument("--message", required=True)
    parser.add_argument("--from", dest="sender", default="")
    parser.add_argument("--base-url", default=os.environ.get("WEBCLX_URL", DEFAULT_URL))
    parser.add_argument("--reply-base-url", default=os.environ.get("WEBCLX_REPLY_URL", ""))
    parser.add_argument("--request-reply", action="store_true")
    parser.add_argument("--wait-ready", action="store_true")
    parser.add_argument("--wait-ready-timeout", type=float, default=120.0)
    parser.add_argument("--no-enter", action="store_true")
    parser.add_argument("--no-verify", action="store_true")
    parser.add_argument("--submit-enters", type=int, default=1)
    parser.add_argument("--raw", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def normalize_url(value: str, option: str) -> str:
    value = value.strip().rstrip("/")
    parsed = urllib.parse.urlparse(value)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname:
        raise RuntimeError(f"{option} must be an http(s) URL with a hostname")
    return value


def is_loopback(url: str) -> bool:
    host = (urllib.parse.urlparse(url).hostname or "").lower()
    return host in {"127.0.0.1", "::1", "localhost"}


def local_auth_headers(base_url: str) -> dict[str, str]:
    if not is_loopback(base_url):
        return {}
    configured = os.environ.get("WEBCLX_LOCAL_TOKEN_FILE", "").strip()
    if not configured:
        return {}
    try:
        token = Path(configured).read_text(encoding="utf-8").strip()
    except OSError as error:
        raise RuntimeError(f"cannot read WEBCLX_LOCAL_TOKEN_FILE: {error}") from error
    if len(token) != 64 or any(character not in "0123456789abcdefABCDEF" for character in token):
        raise RuntimeError("WEBCLX_LOCAL_TOKEN_FILE does not contain a valid local API token")
    return {LOCAL_TOKEN_HEADER: token}


def api_json(base_url: str, path: str, payload: dict | None = None) -> dict:
    url = urllib.parse.urljoin(base_url + "/", path.lstrip("/"))
    data = None
    headers = local_auth_headers(base_url)
    method = "GET"
    if payload is not None:
        data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        headers["Content-Type"] = "application/json"
        method = "POST"
    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with HTTP_OPENER.open(request, timeout=45) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"{error.code} {error.reason}: {detail}") from error


def sessions(base_url: str) -> list[dict]:
    return api_json(base_url, "/api/terminal/sessions?all=true").get("sessions", [])


def clean_path(value: str) -> str:
    return "/".join(part for part in value.strip().strip("/").split("/") if part)


def single_line(value: str, label: str) -> str:
    value = value.replace("\x00", "").replace("\r", "\n")
    value = " ".join(part.strip() for part in value.split("\n") if part.strip())
    if not value:
        raise RuntimeError(f"{label} is empty")
    return value


def session_matches_path(session: dict, path: str) -> bool:
    if not path:
        return True
    expected = clean_path(path)
    actual = clean_path(str(session.get("path") or ""))
    display = clean_path(str(session.get("display_path") or ""))
    return actual == expected or display == expected or display.endswith("/" + expected)


def session_label(session: dict) -> str:
    return f"{session.get('name', '?')} ({session.get('id', '?')}, agent={session.get('activity_agent') or 'shell'})"


def unique(candidates: list[dict], purpose: str) -> dict:
    if len(candidates) == 1:
        return candidates[0]
    if not candidates:
        raise RuntimeError(f"no terminal found for {purpose}")
    labels = ", ".join(session_label(item) for item in candidates)
    raise RuntimeError(f"multiple terminals found for {purpose}: {labels}")


def resolve_target(base_url: str, target: str, agent: str, path: str) -> dict:
    candidates = [item for item in sessions(base_url) if session_matches_path(item, path)]
    if target:
        candidates = [
            item for item in candidates
            if str(item.get("id") or "") == target or str(item.get("name") or "") == target
        ]
        selected = unique(candidates, target)
        detected = str(selected.get("activity_agent") or "").lower()
        if agent and detected != agent:
            raise RuntimeError(f"{session_label(selected)} is not running {agent}")
        return selected
    if not agent:
        raise RuntimeError("pass --target or --agent")
    return unique(
        [item for item in candidates if str(item.get("activity_agent") or "").lower() == agent],
        f"running {agent}",
    )


def infer_sender(base_url: str) -> str:
    session_id = os.environ.get("WEBCLX_TERMINAL_ID", "").strip()
    name = os.environ.get("WEBCLX_TERMINAL_NAME", "").strip()
    if name:
        return name
    if session_id:
        matches = [item for item in sessions(base_url) if str(item.get("id") or "") == session_id]
        return str(unique(matches, session_id).get("name") or session_id)
    raise RuntimeError("cannot infer sender terminal; pass --from")


def verify_reply_target(base_url: str, sender: str) -> None:
    unique(
        [item for item in sessions(base_url) if sender in {str(item.get("id") or ""), str(item.get("name") or "")}],
        f"reply terminal {sender}",
    )


def wait_until_ready(base_url: str, session_id: str, timeout: float) -> None:
    deadline = time.monotonic() + max(timeout, 0.0)
    while True:
        current = next((item for item in sessions(base_url) if str(item.get("id") or "") == session_id), None)
        if current is None:
            raise RuntimeError(f"terminal disappeared: {session_id}")
        if not current.get("busy") and str(current.get("activity_state") or "").lower() != "agent":
            return
        if time.monotonic() >= deadline:
            raise RuntimeError(f"terminal is still busy: {session_label(current)}")
        time.sleep(2)


def run() -> int:
    args = arguments()
    base_url = normalize_url(args.base_url, "--base-url")
    selected = resolve_target(base_url, args.target.strip(), args.agent, args.path)
    target = str(selected.get("id") or "")
    if args.wait_ready:
        wait_until_ready(base_url, target, args.wait_ready_timeout)

    sender = args.sender.strip()
    if not args.raw and not sender:
        sender = infer_sender(args.reply_base_url.strip() or base_url)
    sender = single_line(sender, "sender") if sender else ""
    message = args.message if args.no_enter else single_line(args.message, "message")

    if args.request_reply:
        if args.raw:
            raise RuntimeError("--request-reply cannot be combined with --raw")
        reply_url = args.reply_base_url.strip() or (base_url if is_loopback(base_url) else "")
        if not reply_url:
            raise RuntimeError("remote reply requires --reply-base-url or WEBCLX_REPLY_URL")
        reply_url = normalize_url(reply_url, "--reply-base-url")
        if not is_loopback(base_url) and is_loopback(reply_url):
            raise RuntimeError("a remote destination cannot reply to a loopback URL")
        verify_reply_target(reply_url, sender)
        message += (
            f"; reply with the webclx-terminal-message Skill to {sender} "
            f"through {reply_url}; do not answer only in your own terminal."
        )

    data = message if args.raw else f"[from {sender}] {message}"
    verify = not args.no_enter and not args.no_verify
    payload = {
        "target": target,
        "data": data,
        "submit": not args.no_enter,
        "submit_enters": 0 if args.no_enter else max(1, min(args.submit_enters, 4)),
        "bracketed_paste": not args.no_enter,
        "verify_submission": verify,
        "delivery_id": data if verify else "",
    }
    if args.path:
        payload["path"] = clean_path(args.path)
    if args.dry_run:
        print(json.dumps(payload, ensure_ascii=False))
        return 0
    result = api_json(base_url, "/api/terminal/sessions/message", payload)
    if verify and not result.get("submitted"):
        raise RuntimeError(f"terminal message submission was not confirmed: {json.dumps(result, ensure_ascii=False)}")
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(run())
    except Exception as error:
        print(f"send_terminal_message.py: {error}", file=sys.stderr)
        raise SystemExit(1)
