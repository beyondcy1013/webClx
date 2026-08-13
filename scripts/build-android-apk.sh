#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ANDROID_DIR="${ROOT_DIR}/android"
TOOLCHAIN_DIR="${ANDROID_TOOLCHAIN_DIR:-${ROOT_DIR}/.android-toolchain}"
SDK_ROOT="${ANDROID_HOME:-/home/codes/Android/Sdk}"
BUILD_TOOLS_VERSION="${ANDROID_BUILD_TOOLS_VERSION:-35.0.0}"
GRADLE_VERSION="${ANDROID_GRADLE_VERSION:-8.14.3}"
GRADLE_MIRROR="${ANDROID_GRADLE_MIRROR:-https://mirrors.cloud.tencent.com/gradle}"
GRADLE_ARCHIVE="${TOOLCHAIN_DIR}/gradle-${GRADLE_VERSION}-bin.zip"
GRADLE_HOME="${TOOLCHAIN_DIR}/gradle-${GRADLE_VERSION}"
TMP_DIR="${ROOT_DIR}/.tmp/android-apk"

mkdir -p "${TOOLCHAIN_DIR}" "${TMP_DIR}"
export ANDROID_HOME="${SDK_ROOT}"
export ANDROID_SDK_ROOT="${SDK_ROOT}"

client_version="$(cargo metadata \
    --manifest-path "${ROOT_DIR}/Cargo.toml" \
    --locked \
    --no-deps \
    --format-version 1 | \
    jq -er '.packages[] | select(.name == "webclx") | .version')"

IFS=. read -r version_major version_minor version_patch_extra <<<"${client_version}"
version_patch="${version_patch_extra%%-*}"
if [[ ! "${version_major}" =~ ^[0-9]+$ \
    || ! "${version_minor}" =~ ^[0-9]+$ \
    || ! "${version_patch}" =~ ^[0-9]+$ \
    || "${version_minor}" -ge 1000 \
    || "${version_patch}" -ge 1000 ]]; then
    echo "Unsupported Android version: ${client_version}" >&2
    exit 1
fi
version_code=$((version_major * 1000000 + version_minor * 1000 + version_patch))
if ((version_code <= 0 || version_code > 2100000000)); then
    echo "Android versionCode is out of range: ${version_code}" >&2
    exit 1
fi

required_sdk_files=(
    "${SDK_ROOT}/platforms/android-35/android.jar"
    "${SDK_ROOT}/build-tools/${BUILD_TOOLS_VERSION}/apksigner"
    "${SDK_ROOT}/build-tools/${BUILD_TOOLS_VERSION}/zipalign"
    "${SDK_ROOT}/build-tools/${BUILD_TOOLS_VERSION}/aapt"
)
for required_file in "${required_sdk_files[@]}"; do
    if [[ ! -e "${required_file}" ]]; then
        echo "Missing Android SDK component: ${required_file}" >&2
        exit 1
    fi
done

if [[ ! -s "${GRADLE_ARCHIVE}" ]]; then
    curl --fail --location --retry 3 --retry-all-errors \
        --connect-timeout 15 --max-time 300 \
        "${GRADLE_MIRROR}/gradle-${GRADLE_VERSION}-bin.zip" \
        --output "${GRADLE_ARCHIVE}"
fi
unzip -tq "${GRADLE_ARCHIVE}" >/dev/null
if [[ ! -x "${GRADLE_HOME}/bin/gradle" ]]; then
    unzip -q "${GRADLE_ARCHIVE}" -d "${TOOLCHAIN_DIR}"
fi

"${GRADLE_HOME}/bin/gradle" \
    --no-daemon \
    --stacktrace \
    -p "${ANDROID_DIR}" \
    clean assembleRelease \
    "-PwebclxVersion=${client_version}" \
    "-PwebclxVersionCode=${version_code}"

unsigned_apk="${ANDROID_DIR}/app/build/outputs/apk/release/app-release-unsigned.apk"
if [[ ! -s "${unsigned_apk}" ]]; then
    echo "Android release APK was not produced: ${unsigned_apk}" >&2
    exit 1
fi

password_file="${TOOLCHAIN_DIR}/signing-password"
keystore="${TOOLCHAIN_DIR}/webclx-release.jks"
if [[ ! -s "${password_file}" ]]; then
    umask 077
    openssl rand -hex 24 >"${password_file}"
fi
password="$(<"${password_file}")"
if [[ ! -s "${keystore}" ]]; then
    keytool -genkeypair -noprompt \
        -keystore "${keystore}" \
        -storepass "${password}" \
        -keypass "${password}" \
        -alias webclx \
        -keyalg RSA \
        -keysize 4096 \
        -validity 10000 \
        -dname "CN=webClx, OU=webClx, O=webClx, L=Shanghai, ST=Shanghai, C=CN"
fi
chmod 600 "${password_file}" "${keystore}"

build_tools="${SDK_ROOT}/build-tools/${BUILD_TOOLS_VERSION}"
aligned_apk="${TMP_DIR}/webClx-${client_version}-aligned.apk"
published_dir="${ANDROID_DIR}/app/build/outputs/apk/published"
published_apk="${published_dir}/webClx-${client_version}.apk"
mkdir -p "${published_dir}"

"${build_tools}/zipalign" -f -p 4 "${unsigned_apk}" "${aligned_apk}"
"${build_tools}/apksigner" sign \
    --ks "${keystore}" \
    --ks-key-alias webclx \
    --ks-pass "file:${password_file}" \
    --out "${published_apk}" \
    "${aligned_apk}"
"${build_tools}/apksigner" verify --verbose --print-certs "${published_apk}"
"${build_tools}/aapt" dump badging "${published_apk}" | sed -n '1p'

printf 'APK_PATH=%s\n' "${published_apk}"
printf 'APK_VERSION=%s\n' "${client_version}"
printf 'APK_VERSION_CODE=%s\n' "${version_code}"
printf 'APK_SIZE=%s\n' "$(stat -c '%s' "${published_apk}")"
printf 'APK_SHA256=%s\n' "$(sha256sum "${published_apk}" | cut -d ' ' -f 1)"
