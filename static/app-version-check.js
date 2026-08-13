function getUpdateDownloadUrl() {
  const fallbackOrigin = window.location.origin;
  return state.updateDownloadUrl || `${fallbackOrigin}/api/update/download`;
}

function normalizeVersionText(version) {
  if (typeof version === "number" && Number.isFinite(version)) return String(version);
  return typeof version === "string" ? version.trim() : "";
}

function parseNumericVersionParts(version) {
  const normalized = normalizeVersionText(version);
  if (!normalized) return [];
  return (normalized.match(/\d+/g) || []).map((part) => Number.parseInt(part, 10));
}

function compareNumericVersions(left, right) {
  const leftParts = parseNumericVersionParts(left);
  const rightParts = parseNumericVersionParts(right);
  if (!leftParts.length || !rightParts.length) return null;
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const leftValue = leftParts[index] || 0;
    const rightValue = rightParts[index] || 0;
    if (leftValue > rightValue) return 1;
    if (leftValue < rightValue) return -1;
  }
  return 0;
}

function describeRemoteVersionCheck(remoteVersion, currentVersion) {
  const remote = normalizeVersionText(remoteVersion);
  const current = normalizeVersionText(currentVersion) || "0";
  const comparison = compareNumericVersions(remote, current);
  if (comparison === null) {
    return {
      comparison,
      forceRequired: true,
      message: `无法核对远程版本${remote ? ` ${remote}` : ""}，点击开始更新时需要强制确认。`,
      tone: "warn",
    };
  }
  if (comparison > 0) {
    return {
      comparison,
      forceRequired: false,
      message: `发现新版本: ${remote}`,
      tone: "ok",
    };
  }
  return {
    comparison,
    forceRequired: true,
    message:
      comparison === 0
        ? `远程版本 ${remote} 与当前版本 ${current} 相同；点击开始更新时需要强制确认。`
        : `远程版本 ${remote} 低于当前版本 ${current}；点击开始更新时需要强制确认。`,
    tone: "warn",
  };
}

function confirmForcedRemoteUpdate({ remoteVersion = "", currentVersion = "", comparison = null } = {}) {
  const remote = normalizeVersionText(remoteVersion) || "未知";
  const current = normalizeVersionText(currentVersion) || "未知";
  const reason =
    comparison === 0
      ? "远程版本与当前版本相同"
      : comparison < 0
        ? "远程版本低于当前版本"
        : "无法确认远程版本高于当前版本";
  return window.confirm(
    `${reason}。\n\n当前版本: ${current}\n远程版本: ${remote}\n\n默认只建议更新到更高版本。仍然强制继续吗？`,
  );
}
