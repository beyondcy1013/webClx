#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/hosted-trial-instance.sh --customer-id ID [options]

Options:
  --port PORT             Instance source port (default: 12101)
  --domain DOMAIN         Base domain (default: fpsq.xyz)
  --binary PATH           Verified webClx binary for apply readiness
  --static-dir PATH       Verified static asset directory for apply readiness
  --tls-cert PATH         TLS certificate for the customer hostname
  --tls-key PATH          TLS private key for the customer hostname
  --min-free-gib N        Required free disk space (default: 12)
  --render-dir PATH       Write reviewed systemd/nginx/firewall templates
  --apply                 Run readiness checks for a future provisioning apply
  --confirm ID            Must exactly match --customer-id with --apply

Without --apply this command only prints a secret-free JSON plan.
USAGE
}

CUSTOMER_ID=""
PORT="12101"
DOMAIN="fpsq.xyz"
BINARY=""
STATIC_DIR=""
TLS_CERT=""
TLS_KEY=""
MIN_FREE_GIB="12"
RENDER_DIR=""
APPLY=false
CONFIRM=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --customer-id) CUSTOMER_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --domain) DOMAIN="${2:-}"; shift 2 ;;
    --binary) BINARY="${2:-}"; shift 2 ;;
    --static-dir) STATIC_DIR="${2:-}"; shift 2 ;;
    --tls-cert) TLS_CERT="${2:-}"; shift 2 ;;
    --tls-key) TLS_KEY="${2:-}"; shift 2 ;;
    --min-free-gib) MIN_FREE_GIB="${2:-}"; shift 2 ;;
    --render-dir) RENDER_DIR="${2:-}"; shift 2 ;;
    --apply) APPLY=true; shift ;;
    --confirm) CONFIRM="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unexpected argument: $1" >&2; usage; exit 2 ;;
  esac
done

if [ -z "$CUSTOMER_ID" ]; then
  echo "--customer-id is required" >&2
  exit 2
fi
if ! [[ "$CUSTOMER_ID" =~ ^[a-z][a-z0-9-]{1,30}[a-z0-9]$ ]]; then
  echo "customer id must be 3-32 lowercase letters, digits, or hyphens" >&2
  exit 2
fi
if ! [[ "$PORT" =~ ^[0-9]+$ ]] || [ "$PORT" -lt 1024 ] || [ "$PORT" -gt 65535 ]; then
  echo "port must be an integer from 1024 to 65535" >&2
  exit 2
fi
if ! [[ "$MIN_FREE_GIB" =~ ^[0-9]+$ ]] || [ "$MIN_FREE_GIB" -lt 1 ]; then
  echo "minimum free disk space must be a positive integer" >&2
  exit 2
fi

HOSTNAME="trial-${CUSTOMER_ID}.${DOMAIN}"
SAFE_ID="${CUSTOMER_ID//-/_}"
OS_USER="webclx_${SAFE_ID}"
SERVICE_NAME="webclx-trial-${CUSTOMER_ID}.service"
APP_DIR="/srv/webclx-trials/${CUSTOMER_ID}/app"
WORKSPACE_ROOT="/srv/webclx-trials/${CUSTOMER_ID}/workspace"
ARTIFACT_DIR="/srv/webclx-trials/${CUSTOMER_ID}/artifacts"
BACKUP_TARGET="/srv/webclx-trial-backups/${CUSTOMER_ID}"

if [ -n "$RENDER_DIR" ]; then
  if [ -e "$RENDER_DIR" ] && [ ! -d "$RENDER_DIR" ]; then
    echo "render directory exists and is not a directory: $RENDER_DIR" >&2
    exit 2
  fi
  mkdir -p "$RENDER_DIR"
  cat > "$RENDER_DIR/$SERVICE_NAME" <<EOF
[Unit]
Description=webClx isolated trial for $CUSTOMER_ID
After=network-online.target

[Service]
User=$OS_USER
Group=$OS_USER
WorkingDirectory=$APP_DIR
Environment=WEBCLX_ADDR=0.0.0.0:$PORT
Environment=WEBCLX_STATIC_DIR=$APP_DIR/static
ExecStart=$APP_DIR/webclx serve
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$APP_DIR $WORKSPACE_ROOT $ARTIFACT_DIR
MemoryMax=1G
CPUQuota=100%
TasksMax=256
LimitNOFILE=4096

[Install]
WantedBy=multi-user.target
EOF
  cat > "$RENDER_DIR/nginx-$CUSTOMER_ID.conf" <<EOF
server {
    listen 443 ssl http2;
    server_name $HOSTNAME;
    ssl_certificate $TLS_CERT;
    ssl_certificate_key $TLS_KEY;
    client_max_body_size 16m;

    location / {
        proxy_pass http://127.0.0.1:$PORT;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection \"upgrade\";
    }
}
EOF
  cat > "$RENDER_DIR/firewall-$CUSTOMER_ID.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
iptables -C INPUT -p tcp --dport $PORT ! -s 127.0.0.1 -j REJECT 2>/dev/null || \
  iptables -I INPUT 1 -p tcp --dport $PORT ! -s 127.0.0.1 -j REJECT
EOF
  chmod 0700 "$RENDER_DIR/firewall-$CUSTOMER_ID.sh"
fi

if [ "$APPLY" != true ]; then
  printf '{\n'
  printf '  "mode": "dry-run",\n'
  printf '  "customer_id": "%s",\n' "$CUSTOMER_ID"
  printf '  "hostname": "%s",\n' "$HOSTNAME"
  printf '  "os_user": "%s",\n' "$OS_USER"
  printf '  "service_name": "%s",\n' "$SERVICE_NAME"
  printf '  "loopback_port": %s,\n' "$PORT"
  printf '  "app_dir": "%s",\n' "$APP_DIR"
  printf '  "workspace_root": "%s",\n' "$WORKSPACE_ROOT"
  printf '  "artifact_dir": "%s",\n' "$ARTIFACT_DIR"
  printf '  "backup_target": "%s",\n' "$BACKUP_TARGET"
  printf '  "trial_days": 7,\n'
  printf '  "export_days": 7\n'
  printf '}\n'
  exit 0
fi

if [ "$CONFIRM" != "$CUSTOMER_ID" ]; then
  echo "--confirm must exactly match --customer-id for apply" >&2
  exit 2
fi
if [ ! -x "$BINARY" ]; then
  echo "readiness failed: verified executable binary is required" >&2
  exit 1
fi
if [ ! -f "$STATIC_DIR/index.html" ]; then
  echo "readiness failed: static directory with index.html is required" >&2
  exit 1
fi
if [ ! -r "$TLS_CERT" ] || [ ! -r "$TLS_KEY" ]; then
  echo "readiness failed: readable TLS certificate and key are required" >&2
  exit 1
fi
if ! getent ahosts "$HOSTNAME" >/dev/null 2>&1; then
  echo "readiness failed: DNS does not resolve for $HOSTNAME" >&2
  exit 1
fi
if ss -ltnH "sport = :$PORT" | grep -q .; then
  echo "readiness failed: port $PORT is already in use" >&2
  exit 1
fi

AVAILABLE_KIB="$(df -Pk /srv 2>/dev/null | awk 'NR == 2 {print $4}')"
if [ -z "$AVAILABLE_KIB" ]; then
  AVAILABLE_KIB="$(df -Pk / | awk 'NR == 2 {print $4}')"
fi
REQUIRED_KIB=$((MIN_FREE_GIB * 1024 * 1024))
if [ "$AVAILABLE_KIB" -lt "$REQUIRED_KIB" ]; then
  echo "readiness failed: less than ${MIN_FREE_GIB} GiB free disk space" >&2
  exit 1
fi

echo "readiness checks passed; provisioning apply is intentionally not implemented yet" >&2
exit 3
