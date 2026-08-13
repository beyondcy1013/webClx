// Workspace browser navigation, favorites, entry rendering, and directory loading.
// Loaded before app.js; functions run after app.js globals are initialized.
async function navigateTo(path) {
  state.currentPath = path || "";
  clearEditor("已切换目录，请重新选择文件。");
  await loadDirectory();
}

async function persistFavoritePaths(nextFavorites, successMessage) {

  try {
    const settings = await requestJson("/api/settings", {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        favorite_paths: nextFavorites,
      }),
    });

    state.favoritePaths = settings.favorite_paths;
    renderFavoriteOptions();
    if (state.currentDirectory) {
      renderEntries(state.currentDirectory);
    }
  } catch (error) {
  }
}

function syncWorkspaceShowHiddenInput() {
  if (workspaceShowHiddenInputEl) {
    workspaceShowHiddenInputEl.checked = Boolean(state.showDotEntries);
  }
  if (showDotEntriesInputEl) {
    showDotEntriesInputEl.checked = Boolean(state.showDotEntries);
  }
}

// 工作区多选框与设置页「显示点开头的隐藏文件」共用 show_dot_entries，勾选后立即写回并重载目录。
async function persistWorkspaceShowHidden(nextShowHidden) {
  const previous = Boolean(state.showDotEntries);
  state.showDotEntries = Boolean(nextShowHidden);
  syncWorkspaceShowHiddenInput();

  try {
    const settings = await requestJson("/api/settings", {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        show_dot_entries: Boolean(nextShowHidden),
      }),
    });

    state.showDotEntries = Boolean(settings.show_dot_entries);
    syncWorkspaceShowHiddenInput();
    await loadDirectory();
  } catch (error) {
    state.showDotEntries = previous;
    syncWorkspaceShowHiddenInput();
    updateStatus(fileStatusEl, error.message || "切换隐藏文件显示失败。", "warn");
  }
}

function toggleFavoritePath(absolutePath, kind) {
  if (isFavoritePath(absolutePath)) {
    const nextFavorites = state.favoritePaths.filter((favorite) => favorite.path !== absolutePath);
    persistFavoritePaths(nextFavorites, `已取消收藏：${absolutePath}`);
    return;
  }

  persistFavoritePaths(
    [...state.favoritePaths, { path: absolutePath, kind }],
    `已收藏：${absolutePath}`,
  );
}

async function renameWorkspaceEntry(entry) {
  if (!entry || (entry.kind !== "dir" && entry.kind !== "file")) {
    return;
  }

  const nextName = window.prompt(`重命名${entry.kind === "dir" ? "文件夹" : "文件"}`, entry.name);
  if (nextName === null) {
    return;
  }
  const trimmedName = nextName.trim();
  if (!trimmedName || trimmedName === entry.name) {
    return;
  }

  updateStatus(fileStatusEl, `正在重命名 ${entry.name}…`, "info");
  try {
    const renamed = await requestJson("/api/file/rename", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        path: entry.path,
        name: trimmedName,
      }),
    });

    if (state.currentFilePath) {
      const previousFilePath = state.currentFilePath;
      state.currentFilePath = replaceRelativePathPrefix(
        state.currentFilePath,
        renamed.old_path || entry.path,
        renamed.path || entry.path,
      );
      if (state.currentFilePath === renamed.path) {
        currentFileEl.textContent = renamed.display_path || displayPath(renamed.path);
      } else if (state.currentFilePath !== previousFilePath) {
        currentFileEl.textContent = resolveAbsolutePath(state.workspaceDir, state.currentFilePath);
      }
    }

    const oldAbsolutePath = resolveAbsolutePath(state.workspaceDir, renamed.old_path || entry.path);
    const newAbsolutePath = resolveAbsolutePath(state.workspaceDir, renamed.path || entry.path);
    const favoritePathChanged = state.favoritePaths.some((favorite) => (
      favorite.path === oldAbsolutePath || favorite.path.startsWith(`${oldAbsolutePath}/`)
    ));
    state.favoritePaths = state.favoritePaths.map((favorite) => {
      if (favorite.path === oldAbsolutePath) {
        return { ...favorite, path: newAbsolutePath, kind: renamed.kind || favorite.kind };
      }
      if (favorite.path.startsWith(`${oldAbsolutePath}/`)) {
        const suffix = favorite.path.slice(oldAbsolutePath.length + 1);
        return { ...favorite, path: normalizeAbsolutePath(`${newAbsolutePath}/${suffix}`) };
      }
      return favorite;
    });
    if (favoritePathChanged) {
      await persistFavoritePaths(state.favoritePaths, "");
    } else {
      renderFavoriteOptions();
    }

    await loadDirectory();
    updateStatus(fileStatusEl, `已重命名为 ${trimmedName}。`, "ok");
  } catch (error) {
    updateStatus(fileStatusEl, error.message || "重命名失败。", "warn");
  }
}

async function openFavoritePath(absolutePath) {
  const favorite = state.favoritePaths.find((item) => item.path === absolutePath);
  favoritePathSelectEl.value = "";
  if (!favorite) {
    return;
  }

  const relativePath = relativePathBetweenAbsolute(state.workspaceDir, favorite.path);
  if (favorite.kind === "dir") {
    await navigateTo(relativePath);
    return;
  }

  clearEditor("正在打开收藏文件…");
  state.currentPath = parentRelativePath(relativePath);
  const loaded = await loadDirectory();
  if (loaded) {
    await loadFile(relativePath);
  }
}

function renderEntries(directory) {
  entryList.textContent = "";

  if (directory.parent_path !== null) {
    const row = document.createElement("tr");
    const parentTerminalAction = createActionLink("终端", buildFreshTerminalUrl(directory.parent_path), "mini-button accent");
    parentTerminalAction.addEventListener("click", (event) => {
      recordWorkspaceHistory(directory.parent_path);
      openFreshTerminalLink(event, directory.parent_path);
    });

    let parentDesignateAction;
    parentDesignateAction = createActionButton("指定", () => {
      recordWorkspaceHistory(directory.parent_path);
      openWorkspaceDesignatePresetDialog(directory.parent_path, parentDesignateAction);
    }, "mini-button");

    const actionCell = createActionCell(
      [parentTerminalAction, parentDesignateAction],
      "session-action-cell file-browser-action-cell",
    );
    const favoriteCell = document.createElement("td");
    const iconCell = document.createElement("td");
    iconCell.className = "workspace-icon-cell";
    const nameCell = document.createElement("td");
    nameCell.className = "entry-name";
    const sizeCell = document.createElement("td");
    sizeCell.textContent = "—";

    nameCell.appendChild(
      createEntryLink("..", buildWorkspaceUrl(directory.parent_path), () => navigateTo(directory.parent_path), "entry-link dir"),
    );
    row.append(actionCell, favoriteCell, iconCell, nameCell, sizeCell);
    entryList.appendChild(row);
  }

  if (directory.entries.length === 0) {
    const row = document.createElement("tr");
    row.innerHTML = `<td colspan="5" class="meta-text">当前目录为空。</td>`;
    entryList.appendChild(row);
    return;
  }

  const projectIconColorSlots = workspaceProjectColorSlots(
    directory.entries.filter((entry) => entry.kind === "dir").map((entry) => entry.path),
  );

  directory.entries.forEach((entry) => {
    const row = document.createElement("tr");
    const absolutePath = resolveAbsolutePath(state.workspaceDir, entry.path);
    const isFavorite = isFavoritePath(absolutePath);
    const favoriteLabel = isFavorite ? "★" : "☆";
    const favoriteClass = isFavorite ? "mini-button accent" : "mini-button";

    const nameCell = document.createElement("td");
    nameCell.className = "entry-name";

    const iconCell = document.createElement("td");
    iconCell.className = "workspace-icon-cell";
    if (entry.kind === "dir") {
      const projectIcon = createWorkspaceProjectIcon(
        entry.path,
        state.workspaceBrowserIconPath,
        false,
        "",
        projectIconColorSlots,
      );
      if (projectIcon) {
        iconCell.append(projectIcon);
      }
    }

    const sizeCell = document.createElement("td");
    sizeCell.textContent = entry.kind === "dir" ? "目录" : formatSize(entry.size);

    const favCell = document.createElement("td");
    favCell.className = "fav-cell";
    favCell.appendChild(
      createActionButton(favoriteLabel, () => toggleFavoritePath(absolutePath, entry.kind), favoriteClass),
    );

    let actionCell;
    if (entry.kind === "dir") {
      nameCell.appendChild(
        createEntryLink(entry.name, buildWorkspaceUrl(entry.path), () => navigateTo(entry.path), "entry-link dir"),
      );
      const terminalAction = createActionLink("终端", buildFreshTerminalUrl(entry.path), "mini-button accent");
      terminalAction.addEventListener("click", (event) => {
        recordWorkspaceHistory(entry.path);
        openFreshTerminalLink(event, entry.path);
      });
      let designateAction;
      designateAction = createActionButton("指定", () => {
        recordWorkspaceHistory(entry.path);
        openWorkspaceDesignatePresetDialog(entry.path, designateAction);
      }, "mini-button");
      actionCell = createActionCell(
        [terminalAction, designateAction, createActionButton("改名", () => renameWorkspaceEntry(entry), "mini-button")],
        "session-action-cell file-browser-action-cell",
      );
    } else if (entry.kind === "file") {
      nameCell.appendChild(
        createEntryLink(entry.name, buildWorkspaceUrl(state.currentPath), () => loadFile(entry.path), "entry-link file"),
      );
      actionCell = createActionCell(
        [
          createActionButton("编辑", () => loadFile(entry.path), "mini-button accent"),
          createActionButton("改名", () => renameWorkspaceEntry(entry), "mini-button"),
        ],
        "session-action-cell file-browser-action-cell",
      );
    } else {
      nameCell.textContent = entry.name;
      actionCell = document.createElement("td");
      actionCell.textContent = "暂不支持";
    }

    row.append(actionCell, favCell, iconCell, nameCell, sizeCell);
    entryList.appendChild(row);
  });
}

async function loadDirectory({ allowFallback = true } = {}) {
  state.directorySessions = [];
  state.directorySessionId = "";
  setDirectorySessionPlaceholder(directorySessionLoadingMessage());
  syncSessionsTerminalLink();

  try {
    const directory = await requestJson(`/api/entries?path=${encodeURIComponent(state.currentPath)}`);
    state.currentPath = directory.path;
    state.currentDirectory = directory;
    state.currentWorkspaceDirectoryPath = normalizeAbsolutePath(directory.display_path);
    syncWorkspaceTerminalLink();
    syncDirectorySessionScopeLabel();
    syncSessionsTerminalLink();
    renderCurrentPath(directory.display_path);
    renderEntries(directory);

    syncTabUrl();
    loadDirectorySessions({ preferredSessionId: state.returnTerminalSessionId });
    if (state.activeTab === "workspace") {
      loadSessions();
    }
    return true;
  } catch (error) {
    if (
      allowFallback &&
      state.currentPath &&
      (error.message.includes("路径不存在") || error.message.includes("目录不存在"))
    ) {
      state.currentPath = "";
      state.currentDirectory = null;
      clearEditor("路径不存在，已切换到默认工作目录。");
      renderCurrentPath(state.workspaceDir || "/");
      window.history.replaceState({}, "", buildWorkspaceUrl(""));
      state.directorySessions = [];
      state.directorySessionId = "";
      syncWorkspaceTerminalLink();
      syncSessionsTerminalLink();
      const loaded = await loadDirectory({ allowFallback: false });

      return loaded;
    }

    state.currentDirectory = null;
    state.directorySessions = [];
    state.directorySessionId = "";
    entryList.textContent = "";
    setDirectorySessionPlaceholder("目录不可用，无法读取当前目录终端会话。");
    if (state.activeTab === "workspace") {
      loadSessions();
    }
    return false;
  }
}
