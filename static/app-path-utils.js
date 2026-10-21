// 路径操作、收藏路径判断与文件大小格式化工具函数模块。
// 由 app.js 拆出，在 app.js 之前以 <script defer> 加载，
// 通过共享全局作用域向 app.js 提供下列函数，无需修改调用方。
// 依赖的全局（state.*、favoritePathSelectEl）均为 app.js 顶层声明，加载顺序保证可用。

function splitPathParts(path) {
  return String(path || "")
    .split("/")
    .filter(Boolean);
}

function normalizeRelativePath(path) {
  const normalized = [];
  splitPathParts(path).forEach((part) => {
    if (part === ".") {
      return;
    }
    if (part === "..") {
      if (normalized.length > 0 && normalized[normalized.length - 1] !== "..") {
        normalized.pop();
      } else {
        normalized.push(part);
      }
      return;
    }
    normalized.push(part);
  });
  return normalized.join("/");
}

function normalizeAbsolutePath(path) {
  const normalized = [];
  splitPathParts(path).forEach((part) => {
    if (part === ".") {
      return;
    }
    if (part === "..") {
      normalized.pop();
      return;
    }
    normalized.push(part);
  });
  return `/${normalized.join("/")}`;
}

function resolveAbsolutePath(basePath, relativePath) {
  if (!relativePath) {
    return normalizeAbsolutePath(basePath);
  }
  if (relativePath.startsWith("/")) {
    return normalizeAbsolutePath(relativePath);
  }

  const resolved = splitPathParts(basePath);
  splitPathParts(relativePath).forEach((part) => {
    if (part === ".") {
      return;
    }
    if (part === "..") {
      resolved.pop();
      return;
    }
    resolved.push(part);
  });
  return `/${resolved.join("/")}`;
}

function relativePathBetweenAbsolute(basePath, targetPath) {
  const baseParts = splitPathParts(normalizeAbsolutePath(basePath));
  const targetParts = splitPathParts(normalizeAbsolutePath(targetPath));
  let commonLength = 0;

  while (
    commonLength < baseParts.length &&
    commonLength < targetParts.length &&
    baseParts[commonLength] === targetParts[commonLength]
  ) {
    commonLength += 1;
  }

  const relativeParts = [
    ...new Array(baseParts.length - commonLength).fill(".."),
    ...targetParts.slice(commonLength),
  ];
  return relativeParts.join("/");
}

function parentRelativePath(path) {
  const parts = splitPathParts(path);
  if (parts.length === 0) {
    return "";
  }
  parts.pop();
  return parts.join("/");
}

function replaceRelativePathPrefix(path, oldPrefix, newPrefix) {
  const normalizedPath = normalizeRelativePath(path);
  const normalizedOldPrefix = normalizeRelativePath(oldPrefix);
  const normalizedNewPrefix = normalizeRelativePath(newPrefix);
  if (normalizedPath === normalizedOldPrefix) {
    return normalizedNewPrefix;
  }
  if (normalizedOldPrefix && normalizedPath.startsWith(`${normalizedOldPrefix}/`)) {
    const suffix = normalizedPath.slice(normalizedOldPrefix.length + 1);
    return normalizeRelativePath(`${normalizedNewPrefix}/${suffix}`);
  }
  return path;
}

function isFavoritePath(path) {
  return state.favoritePaths.some((favorite) => favorite.path === path);
}

function renderFavoriteOptions() {
  favoritePathSelectEl.textContent = "";

  const placeholder = document.createElement("option");
  placeholder.value = "";
  placeholder.textContent = state.favoritePaths.length > 0 ? "跳转收藏" : "暂无收藏路径";
  favoritePathSelectEl.appendChild(placeholder);

  state.favoritePaths.forEach((favorite) => {
    const option = document.createElement("option");
    option.value = favorite.path;
    option.textContent = favorite.path;
    favoritePathSelectEl.appendChild(option);
  });

  favoritePathSelectEl.value = "";
  favoritePathSelectEl.disabled = state.favoritePaths.length === 0;
}

function formatSize(size) {
  if (size === null || size === undefined) {
    return "—";
  }

  const units = ["B", "KB", "MB", "GB"];
  let value = size;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }

  return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}
