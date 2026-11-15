#!/usr/bin/env bash
# Compare a fresh release against the published APK before any publisher runs.
set -euo pipefail
fail() { printf 'Android upgrade identity check failed: %s\n' "$*" >&2; exit 1; }
[[ $# == 2 ]] || { echo 'usage: check-android-upgrade-identity.sh OLD_APK NEW_APK' >&2; exit 2; }
TOOLS="${ANDROID_HOME:-/home/codes/Android/Sdk}/build-tools/35.0.0"
old_badging="$("$TOOLS/aapt" dump badging "$1")"
new_badging="$("$TOOLS/aapt" dump badging "$2")"
package_id() { sed -n "s/^package: name='\([^']*\)'.*/\1/p"; }
version_code() { sed -n "s/^package: name='[^']*' versionCode='\([^']*\)'.*/\1/p"; }
old_package="$(package_id <<< "$old_badging")"
new_package="$(package_id <<< "$new_badging")"
old_code="$(version_code <<< "$old_badging")"
new_code="$(version_code <<< "$new_badging")"
[[ -n "$old_package" && "$old_package" == "$new_package" ]] || fail "package $old_package -> $new_package"
[[ "$old_code" =~ ^[0-9]+$ && "$new_code" =~ ^[0-9]+$ ]] || fail 'missing numeric versionCode'
(( new_code > old_code )) || fail "versionCode must increase: $old_code -> $new_code"
old_certs="$("$TOOLS/apksigner" verify --print-certs "$1")"
new_certs="$("$TOOLS/apksigner" verify --print-certs "$2")"
certs() { sed -n '/^Signer #[0-9]* certificate SHA-256 digest:/p'; }
old_digest="$(certs <<< "$old_certs")"
new_digest="$(certs <<< "$new_certs")"
[[ -n "$old_digest" && "$old_digest" == "$new_digest" ]] || fail "signer mismatch: old=$old_digest new=$new_digest"
! [[ "$new_badging" == *application-debuggable* ]] || fail 'release APK is debuggable'
"$TOOLS/zipalign" -c -p 4 "$2"
printf 'Upgrade identity verified: %s %s -> %s\n%s\n' "$new_package" "$old_code" "$new_code" "$new_digest"
