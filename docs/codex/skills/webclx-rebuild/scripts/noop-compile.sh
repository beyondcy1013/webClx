#!/usr/bin/env bash
# No-op compile stage. Used by the webClx deploy worker when a project ships a
# self-hosted scripts/rebuild-and-deploy.sh that performs its own build (e.g.
# Windows cross-compile targets). Running a host-native `cargo build --release`
# first would either waste time compiling an unused native binary or fail on
# missing system libraries (atk, gdk-pixbuf, ...). This script lets the install
# stage (rebuild-and-deploy.sh) own both compile and deploy.
set -euo pipefail
:
