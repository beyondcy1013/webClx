#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/hosted-trial-qa-lifecycle.sh ACTION --customer-id qa-ID [options]

Actions:
  provision   Create an isolated, loopback-only QA instance
  freeze      Stop the instance and make its workspace read-only
  export      Archive only the frozen workspace
  delete      Remove the instance and verify that no runtime residue remains

Options:
  --port PORT             Instance port (default: 12101)
  --binary PATH           Verified webClx executable for provision
  --static-dir PATH       Static assets containing index.html for provision
  --root-dir PATH         Instance root (default: /srv/webclx-trials)
  --export-dir PATH       Export root (default: /srv/webclx-trial-exports)
  --apply                 Perform operations; otherwise print a dry-run plan
  --dry-run               Explicitly print a dry-run plan
  --confirm-delete ID     Must exactly match --customer-id for delete
  --keep-export           Preserve exports when deleting an instance
USAGE
}

die_usage() {
  echo "$1" >&2
  exit 2
}

shell_quote() {
  printf '%q' "$1"
}

print_command() {
  local separator=""
  local argument
  for argument in "$@"; do
    printf '%s' "$separator"
    shell_quote "$argument"
    separator=" "
  done
  printf '\n'
}

run() {
  if [ "$APPLY" = true ]; then
    "$@"
  else
    print_command "$@"
  fi
}

run_if_present() {
  if [ "$APPLY" = true ]; then
    "$@" 2>/dev/null || true
  else
    print_command "$@"
  fi
}

write_manifest() {
  local state="$1"
  local now
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  umask 077
  cat > "$MANIFEST.tmp" <<EOF
customer_id=$CUSTOMER_ID
os_user=$OS_USER
service_name=$SERVICE_NAME
port=$PORT
instance_dir=$INSTANCE_DIR
app_dir=$APP_DIR
workspace_dir=$WORKSPACE_DIR
artifact_dir=$ARTIFACT_DIR
export_dir=$CUSTOMER_EXPORT_DIR
state=$state
updated_at=$now
EOF
  mv "$MANIFEST.tmp" "$MANIFEST"
}

load_manifest() {
  [ -f "$MANIFEST" ] || { echo "instance manifest not found: $MANIFEST" >&2; exit 1; }
  local key value
  while IFS='=' read -r key value; do
    case "$key" in
      customer_id) [ "$value" = "$CUSTOMER_ID" ] || { echo "manifest customer mismatch" >&2; exit 1; } ;;
      port) PORT="$value" ;;
      os_user) OS_USER="$value" ;;
      service_name) SERVICE_NAME="$value" ;;
      instance_dir) [ "$value" = "$INSTANCE_DIR" ] || { echo "manifest path mismatch" >&2; exit 1; } ;;
      app_dir) APP_DIR="$value" ;;
      workspace_dir) WORKSPACE_DIR="$value" ;;
      artifact_dir) ARTIFACT_DIR="$value" ;;
      export_dir) CUSTOMER_EXPORT_DIR="$value" ;;
    esac
  done < "$MANIFEST"
}

validate_safe_directory() {
  local directory="$1"
  local expected_parent="$2"
  [ -n "$directory" ] || die_usage "directory must not be empty"
  [ "$directory" != "/" ] || die_usage "refusing to use the filesystem root"
  case "$directory" in
    "$expected_parent"/*) ;;
    *) die_usage "directory escapes its configured root: $directory" ;;
  esac
}

ACTION="${1:-}"
case "$ACTION" in
  provision|freeze|export|delete) shift ;;
  -h|--help) usage; exit 0 ;;
  *) usage; exit 2 ;;
esac

CUSTOMER_ID=""
PORT="12101"
BINARY=""
STATIC_DIR=""
ROOT_DIR="/srv/webclx-trials"
EXPORT_DIR="/srv/webclx-trial-exports"
APPLY=false
CONFIRM_DELETE=""
KEEP_EXPORT=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --customer-id) CUSTOMER_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --binary) BINARY="${2:-}"; shift 2 ;;
    --static-dir) STATIC_DIR="${2:-}"; shift 2 ;;
    --root-dir) ROOT_DIR="${2:-}"; shift 2 ;;
    --export-dir) EXPORT_DIR="${2:-}"; shift 2 ;;
    --apply) APPLY=true; shift ;;
    --dry-run) APPLY=false; shift ;;
    --confirm-delete) CONFIRM_DELETE="${2:-}"; shift 2 ;;
    --keep-export) KEEP_EXPORT=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die_usage "unexpected argument: $1" ;;
  esac
done

[ -n "$CUSTOMER_ID" ] || die_usage "--customer-id is required"
[[ "$CUSTOMER_ID" =~ ^qa-[a-z0-9][a-z0-9-]{0,25}[a-z0-9]$ ]] || \
  die_usage "QA customer id must begin with qa- and contain 5-29 lowercase letters, digits, or hyphens"
[[ "$PORT" =~ ^[0-9]+$ ]] && [ "$PORT" -ge 1024 ] && [ "$PORT" -le 65535 ] || \
  die_usage "port must be an integer from 1024 to 65535"
[[ "$ROOT_DIR" = /* && "$EXPORT_DIR" = /* ]] || die_usage "root and export directories must be absolute"
case "/$ROOT_DIR/$EXPORT_DIR/" in
  */../*|*/./*) die_usage "root and export directories must not contain dot path components" ;;
esac

SAFE_ID="${CUSTOMER_ID//-/_}"
OS_USER="webclx_${SAFE_ID}"
SERVICE_NAME="webclx-qa-${CUSTOMER_ID}.service"
INSTANCE_DIR="$ROOT_DIR/$CUSTOMER_ID"
APP_DIR="$INSTANCE_DIR/app"
WORKSPACE_DIR="$INSTANCE_DIR/workspace"
ARTIFACT_DIR="$INSTANCE_DIR/artifacts"
MANIFEST="$INSTANCE_DIR/manifest.env"
CUSTOMER_EXPORT_DIR="$EXPORT_DIR/$CUSTOMER_ID"
UNIT_PATH="/etc/systemd/system/$SERVICE_NAME"
FIREWALL_COMMENT="webclx-qa:$CUSTOMER_ID"

validate_safe_directory "$INSTANCE_DIR" "$ROOT_DIR"
validate_safe_directory "$CUSTOMER_EXPORT_DIR" "$EXPORT_DIR"

case "$ACTION" in
  provision)
    [ -n "$BINARY" ] || die_usage "--binary is required for provision"
    [ -n "$STATIC_DIR" ] || die_usage "--static-dir is required for provision"
    if [ "$APPLY" = true ]; then
      [ -x "$BINARY" ] || { echo "verified executable binary is required" >&2; exit 1; }
      [ -f "$STATIC_DIR/index.html" ] || { echo "static directory with index.html is required" >&2; exit 1; }
      [ ! -L "$BINARY" ] && [ ! -L "$STATIC_DIR" ] || { echo "binary and static directory must not be symbolic links" >&2; exit 1; }
      [ ! -e "$INSTANCE_DIR" ] || { echo "instance already exists: $INSTANCE_DIR" >&2; exit 1; }
      ! ss -ltnH "sport = :$PORT" | grep -q . || { echo "port is already in use: $PORT" >&2; exit 1; }
    fi
    run useradd --system --home-dir "$INSTANCE_DIR" --shell /usr/sbin/nologin "$OS_USER"
    run install -d -m 0750 -o "$OS_USER" -g "$OS_USER" "$APP_DIR" "$WORKSPACE_DIR" "$ARTIFACT_DIR"
    run install -m 0750 -o root -g "$OS_USER" "$BINARY" "$APP_DIR/webclx"
    run cp -a --no-dereference "$STATIC_DIR" "$APP_DIR/static"
    if [ "$APPLY" = true ]; then
      chown -R root:"$OS_USER" "$APP_DIR/static"
      chmod -R go-w "$APP_DIR/static"
      write_manifest provisioned
      cat > "$UNIT_PATH" <<EOF
[Unit]
Description=webClx isolated QA instance $CUSTOMER_ID
After=network-online.target

[Service]
User=$OS_USER
Group=$OS_USER
WorkingDirectory=$APP_DIR
Environment=WEBCLX_ADDR=127.0.0.1:$PORT
Environment=WEBCLX_STATIC_DIR=$APP_DIR/static
ExecStart=$APP_DIR/webclx serve
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$APP_DIR $WORKSPACE_DIR $ARTIFACT_DIR
MemoryMax=1G
CPUQuota=100%
TasksMax=256
LimitNOFILE=4096

[Install]
WantedBy=multi-user.target
EOF
      chmod 0644 "$UNIT_PATH"
    else
      print_command install -m 0644 /dev/stdin "$UNIT_PATH"
    fi
    run iptables -I INPUT 1 -p tcp --dport "$PORT" ! -s 127.0.0.1 -m comment --comment "$FIREWALL_COMMENT" -j REJECT
    run systemctl daemon-reload
    run systemctl enable --now "$SERVICE_NAME"
    ;;

  freeze)
    if [ "$APPLY" = true ]; then load_manifest; fi
    run systemctl stop "$SERVICE_NAME"
    run chmod -R a-w "$WORKSPACE_DIR"
    if [ "$APPLY" = true ]; then write_manifest frozen; fi
    ;;

  export)
    if [ "$APPLY" = true ]; then
      load_manifest
      [ -d "$WORKSPACE_DIR" ] && [ ! -L "$WORKSPACE_DIR" ] || { echo "workspace must be a real directory" >&2; exit 1; }
      if find "$WORKSPACE_DIR" -type l -print -quit | grep -q .; then
        echo "workspace export refuses symbolic links" >&2
        exit 1
      fi
    fi
    ARCHIVE="$CUSTOMER_EXPORT_DIR/${CUSTOMER_ID}-workspace.tar.gz"
    run install -d -m 0700 "$CUSTOMER_EXPORT_DIR"
    run tar --create --gzip --file "$ARCHIVE" --directory "$INSTANCE_DIR" workspace
    if [ "$APPLY" = true ]; then write_manifest exported; fi
    ;;

  delete)
    [ "$CONFIRM_DELETE" = "$CUSTOMER_ID" ] || die_usage "--confirm-delete must exactly match --customer-id"
    if [ "$APPLY" = true ] && [ -f "$MANIFEST" ]; then load_manifest; fi
    run_if_present systemctl disable --now "$SERVICE_NAME"
    run_if_present iptables -D INPUT -p tcp --dport "$PORT" ! -s 127.0.0.1 -m comment --comment "$FIREWALL_COMMENT" -j REJECT
    run rm -f -- "$UNIT_PATH"
    run systemctl daemon-reload
    run_if_present userdel "$OS_USER"
    run rm -rf --one-file-system -- "$INSTANCE_DIR"
    if [ "$KEEP_EXPORT" != true ]; then
      run rm -rf --one-file-system -- "$CUSTOMER_EXPORT_DIR"
    fi
    if [ "$APPLY" = true ]; then
      [ ! -e "$INSTANCE_DIR" ] || { echo "residue remains: $INSTANCE_DIR" >&2; exit 1; }
      [ ! -e "$UNIT_PATH" ] || { echo "residue remains: $UNIT_PATH" >&2; exit 1; }
      ! id "$OS_USER" >/dev/null 2>&1 || { echo "residue remains: user $OS_USER" >&2; exit 1; }
      ! ss -ltnH "sport = :$PORT" | grep -q . || { echo "residue remains: port $PORT" >&2; exit 1; }
      ! iptables -C INPUT -p tcp --dport "$PORT" ! -s 127.0.0.1 -m comment --comment "$FIREWALL_COMMENT" -j REJECT 2>/dev/null || \
        { echo "residue remains: firewall rule" >&2; exit 1; }
    fi
    ;;
esac
