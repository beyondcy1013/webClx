#!/usr/bin/env bash
# Publish verified APKs to the existing CN HTTPS Android repository.
set -euo pipefail
[[ $# == 3 ]] || { echo 'usage: publish-android-https-update.sh PROJECT APK RELEASE_NOTES' >&2; exit 2; }
project="$1"; apk="$2"; notes="$3"
[[ "$project" =~ ^[A-Za-z0-9_-]+$ && -s "$apk" && -n "${notes//[[:space:]]/}" ]] || exit 2
sdk_tools="${ANDROID_HOME:-/home/codes/Android/Sdk}/build-tools/35.0.0"
badging="$("$sdk_tools/aapt" dump badging "$apk")"
version="$(sed -n "s/^package: .* versionName='\([^']*\)'.*/\1/p" <<< "$badging")"
code="$(sed -n "s/^package: .* versionCode='\([^']*\)'.*/\1/p" <<< "$badging")"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && "$code" =~ ^[1-9][0-9]*$ ]] || exit 2
! [[ "$badging" == *application-debuggable* ]]
signatures="$("$sdk_tools/apksigner" verify --min-sdk-version 23 --verbose "$apk")"
[[ "$signatures" == *'Verified using v1 scheme (JAR signing): true'* && "$signatures" == *'Verified using v2 scheme (APK Signature Scheme v2): true'* ]]
"$sdk_tools/zipalign" -c -p 4 "$apk"
sha="$(sha256sum "$apk" | cut -d ' ' -f1)"
size="$(stat -c '%s' "$apk")"
name="$project-$version.apk"
base='https://fpsq.xyz:11113/android-updates'
remote='ubuntu@xs.fpsq.xyz'
repository='/var/lib/webclx-android-updates'
stage="/home/ubuntu/.cache/android-publish-$project-$(date +%s%N)-$$"
manifest="$(mktemp)"
trap 'rm -f "$manifest"' EXIT
jq -n --arg version "$version" --argjson version_code "$code" \
  --arg file "$name" --arg sha256 "$sha" --argjson size "$size" \
  --arg download_url "$base/$name" --arg release_notes "$notes" \
  --arg published_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{version:$version,version_code:$version_code,mandatory:false,platform:"android",arch:"universal",file:$file,sha256:$sha256,size:$size,download_url:$download_url,release_notes:$release_notes,published_at:$published_at}' > "$manifest"
ssh -o BatchMode=yes -o ConnectTimeout=8 "$remote" "install -d -m 0700 '$stage'"
scp -q "$apk" "$remote:$stage/$name"
scp -q "$manifest" "$remote:$stage/$project.json"
ssh -o BatchMode=yes -o ConnectTimeout=8 "$remote" \
  "sudo bash -s -- '$stage' '$repository' '$project' '$name' '$sha' '$code'" <<'REMOTE'
set -euo pipefail
stage="$1"; repository="$2"; project="$3"; name="$4"; expected="$5"; candidate="$6"
exec 9>"$repository/.${project}.publish.lock"
flock -x 9
actual="$(sha256sum "$stage/$name" | cut -d ' ' -f1)"
[[ "$actual" == "$expected" ]]
current=0
if [[ -f "$repository/$project.json" ]]; then
  current="$(jq -er '.version_code' "$repository/$project.json")"
fi
[[ "$current" =~ ^[0-9]+$ ]]
(( candidate >= current )) || { echo 'refusing Android update rollback' >&2; exit 1; }
if (( candidate == current )); then
  [[ "$(jq -er '.sha256' "$repository/$project.json")" == "$expected" ]]
fi
if [[ -f "$repository/$name" ]]; then
  [[ "$(sha256sum "$repository/$name" | cut -d ' ' -f1)" == "$expected" ]]
else
  install -o root -g www-data -m 0644 "$stage/$name" "$repository/$name.part"
  mv "$repository/$name.part" "$repository/$name"
fi
install -o root -g www-data -m 0644 "$stage/$project.json" "$repository/$project.json.part"
mv "$repository/$project.json.part" "$repository/$project.json"
rm -f "$stage/$name" "$stage/$project.json"
rmdir "$stage"
REMOTE
download="$(mktemp)"
trap 'rm -f "$manifest" "$download"' EXIT
curl --noproxy '*' -fsS --max-time 30 "$base/$project.json" | \
  jq -e --arg v "$version" --arg h "$sha" --argjson c "$code" --argjson s "$size" \
  '.version == $v and .version_code == $c and .sha256 == $h and .size == $s' > /dev/null
curl --noproxy '*' -fsS --max-time 180 "$base/$name" -o "$download"
[[ "$(stat -c '%s' "$download")" == "$size" && "$(sha256sum "$download" | cut -d ' ' -f1)" == "$sha" ]]
printf 'HTTPS_MANIFEST=%s\nHTTPS_APK=%s\nSHA256=%s\n' "$base/$project.json" "$base/$name" "$sha"
