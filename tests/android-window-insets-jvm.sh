#!/usr/bin/env bash
# Run through the compile API; the fixtures model inset delivery, not Android rendering.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURES="$REPO_ROOT/tests/fixtures/android-window-insets"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

SOURCE="${1:-$REPO_ROOT/android/app/src/main/java/com/webclx/app/WindowInsetsLayout.java}"
PACKAGE="$(sed -n 's/^package \([a-zA-Z0-9_.]*\);$/\1/p' "$SOURCE")"
[[ "$PACKAGE" =~ ^[a-zA-Z_][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)*$ ]]
sed "s/^package com.webclx.app;/package $PACKAGE;/" \
  "$FIXTURES/com/webclx/app/WindowInsetsHarness.java" > "$OUT/WindowInsetsHarness.java"

javac -d "$OUT" \
  $(find "$FIXTURES" -name '*.java' ! -name 'WindowInsetsHarness.java' | sort) \
  "$OUT/WindowInsetsHarness.java" "$SOURCE"
java -cp "$OUT" "$PACKAGE.WindowInsetsHarness"
