#!/usr/bin/env bash
# JVM regression harness for the real UpdateManager update-transaction guards.
# Compiles android/app/.../UpdateManager.java against the minimal Android stubs
# in tests/fixtures/android-update-state/ and runs UpdateStateHarness.
#
# This is a build step: run it through the release queue together with the
# Android build, not as part of the local JS test gate.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURES="$REPO_ROOT/tests/fixtures/android-update-state"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

javac -d "$OUT" \
  $(find "$FIXTURES" -name '*.java' | sort) \
  "$REPO_ROOT/android/app/src/main/java/com/webclx/app/UpdateManager.java"

java -cp "$OUT" com.webclx.app.UpdateStateHarness "$OUT/work"
