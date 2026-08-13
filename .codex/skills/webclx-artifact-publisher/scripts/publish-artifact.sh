#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${WEBCLX_BASE_URL:-http://127.0.0.1:11111}"
PROJECT=""
ARTIFACT_PATH=""
NAME=""
LABEL=""
NOTE=""
VERSION=""
LOCAL_TOKEN_FILE="${WEBCLX_LOCAL_TOKEN_FILE:-}"

usage() {
  cat <<'USAGE'
Usage: publish-artifact.sh --path <absolute-file> [--project name] [--version version] [--name file] [--label label] [--note note] [--base-url url]
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url)
      BASE_URL="${2:-}"; shift 2 ;;
    --project)
      PROJECT="${2:-}"; shift 2 ;;
    --path)
      ARTIFACT_PATH="${2:-}"; shift 2 ;;
    --version)
      VERSION="${2:-}"; shift 2 ;;
    --name)
      NAME="${2:-}"; shift 2 ;;
    --label)
      LABEL="${2:-}"; shift 2 ;;
    --note)
      NOTE="${2:-}"; shift 2 ;;
    --help|-h)
      usage; exit 0 ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2 ;;
  esac
done

if [[ -z "${ARTIFACT_PATH}" ]]; then
  echo "--path is required" >&2
  exit 2
fi
if [[ "${ARTIFACT_PATH}" != /* ]]; then
  echo "--path must be absolute: ${ARTIFACT_PATH}" >&2
  exit 2
fi
if [[ ! -f "${ARTIFACT_PATH}" ]]; then
  echo "artifact does not exist: ${ARTIFACT_PATH}" >&2
  exit 2
fi
if [[ -z "${PROJECT}" ]]; then
  PROJECT="$(basename "$(pwd -P)")"
fi
artifact_basename="$(basename "${ARTIFACT_PATH}")"
artifact_extension="${artifact_basename##*.}"
if [[ "${artifact_extension,,}" == "apk" ]]; then
  if [[ -z "${VERSION}" ]]; then
    echo "--version is required when publishing an APK" >&2
    exit 2
  fi
  if [[ "${VERSION}" == */* ]]; then
    echo "--version must not contain a path separator: ${VERSION}" >&2
    exit 2
  fi
  if [[ -z "${NAME}" ]]; then
    artifact_stem="${artifact_basename%.*}"
    if [[ "${artifact_stem}" == *"${VERSION}"* ]]; then
      NAME="${artifact_basename}"
    else
      NAME="${artifact_stem}-${VERSION}.apk"
    fi
  fi
  if [[ "${NAME,,}" != *.apk ]]; then
    echo "APK download name must end with .apk: ${NAME}" >&2
    exit 2
  fi
  if [[ "${NAME}" != *"${VERSION}"* ]]; then
    echo "APK download name must include version ${VERSION}: ${NAME}" >&2
    exit 2
  fi
elif [[ -z "${NAME}" ]]; then
  NAME="${artifact_basename}"
fi
if [[ -z "${LABEL}" ]]; then
  LABEL="${NAME}"
fi

python3 - "$BASE_URL" "$PROJECT" "$ARTIFACT_PATH" "$NAME" "$LABEL" "$NOTE" "$VERSION" "$LOCAL_TOKEN_FILE" <<'PY'
import json
import sys
import urllib.error
import urllib.parse
import urllib.request

class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

http_opener = urllib.request.build_opener(NoRedirectHandler())

base_url, project, artifact_path, name, label, note, version, local_token_file = sys.argv[1:]
base_url = base_url.rstrip("/")
parsed_base = urllib.parse.urlparse(base_url)
headers = {"content-type": "application/json"}
if (parsed_base.hostname or "").lower() in {"127.0.0.1", "::1", "localhost"}:
    token = ""
    if local_token_file:
        try:
            token = open(local_token_file, encoding="utf-8").read().strip()
        except OSError:
            token = ""
    if len(token) == 64 and all(character in "0123456789abcdefABCDEF" for character in token):
        headers["X-WebClx-Local-Token"] = token
payload = {
    "project": project,
    "path": artifact_path,
    "name": name,
    "label": label,
    "note": note,
}
body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
request = urllib.request.Request(
    f"{base_url}/api/artifacts/publish",
    data=body,
    headers=headers,
    method="POST",
)
try:
    with http_opener.open(request, timeout=30) as response:
        data = json.loads(response.read().decode("utf-8"))
except urllib.error.HTTPError as error:
    detail = error.read().decode("utf-8", errors="replace")
    raise SystemExit(f"webClx publish failed: HTTP {error.code}: {detail}")
except Exception as error:
    raise SystemExit(f"webClx publish failed: {error}")

data["absolute_download_url"] = base_url + data.get("download_url", "")
data["downloads_page_url"] = base_url + "/downloads"
if artifact_path.lower().endswith(".apk"):
    manifest_url = f"{base_url}/api/artifacts/update/android/{urllib.parse.quote(project, safe='')}"
    try:
        with http_opener.open(manifest_url, timeout=30) as response:
            manifest = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise SystemExit(f"webClx update-center verification failed: HTTP {error.code}: {detail}")
    except Exception as error:
        raise SystemExit(f"webClx update-center verification failed: {error}")
    expected = {
        "version": version,
        "file": data.get("name"),
        "sha256": data.get("sha256"),
        "size": data.get("size"),
        "download_url": data.get("download_url"),
    }
    mismatches = {
        key: {"expected": value, "actual": manifest.get(key)}
        for key, value in expected.items()
        if manifest.get(key) != value
    }
    if mismatches:
        raise SystemExit(
            "webClx update-center verification failed: "
            + json.dumps(mismatches, ensure_ascii=False)
        )
    data["update_manifest_url"] = manifest_url
    data["update_manifest"] = manifest
print(json.dumps(data, ensure_ascii=False, indent=2))
PY
