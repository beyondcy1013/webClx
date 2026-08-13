# 需求文档：前端 WebUI 独立 Tab URL 支持

## 1. 需求背景
目前 webClx 目录下的 WebUI 在点击设置（Settings）页面下的不同子 Tab（如 Workspace, Appearance, Compile, Proxy 等）时，浏览器 URL 哈希全部共享 `#settings`。
这导致用户无法通过特定的 URL 直接定位到特定的设置子面板，刷新页面后也会丢失当前处于哪个设置子面板的状态。

为了提升用户体验，需要确保所有 Tab（包括主 Tab 及 Settings 子 Tab）都有独立的 URL，并能在页面加载或 URL 变更时实现正确的状态还原。

## 2. 功能设计
1. **URL 哈希路由规范**:
   - 主 Tab: 继承现有路由（如 `#history`、`#sessions`、`#auth` 等）。
   - 设置子 Tab: 使用层级结构 `#settings/<sub-tab-name>`。
     - 例如：设置代理面板哈希为 `#settings/proxy`，设置编译面板哈希为 `#settings/compile`。
     - 如果哈希为 `#settings`，默认映射到 `#settings/workspace`。

2. **状态双向同步**:
   - **界面 -> URL**: 每次用户点击切换主 Tab 或设置子 Tab，都自动且无感地将最新的哈希更新到浏览器的 URL 地址栏（为兼容历史行为，采用 `history.replaceState` 防止产生过多历史记录栈）。
   - **URL -> 界面**: 
     - 页面初始化时，解析 URL 中的哈希。如果是 `#settings/<sub-tab-name>`，同时激活 `settings` 主 Tab 以及对应的设置子 Tab。
     - 注册 `hashchange` 监听器。当哈希发生手动变更（如地址栏修改、书签、第三方跳转）时，界面随之自动切换到指定 Tab。

## 3. 实现计划

### 3.1 `app-navigation-tabs.js` 调整
- 修改 `currentTabHash()`：
  ```javascript
  if (state.activeTab === "settings") {
    return `#settings/${state.activeSettingsTab || "workspace"}`;
  }
  ```
- 修改 `setActiveSettingsTab(tab)`，在末尾加入：
  ```javascript
  if (state.activeTab === "settings") {
    syncTabUrl();
  }
  ```

### 3.2 `app.js` 调整
- 修改 `getInitialTab()` 支持 `#settings/` 开头的哈希识别并返回 `"settings"`。
- 新增 `getInitialSettingsTab()` 从 URL 中提取并校验子 Tab 的合法性。
- 修改 `state` 初始化，设置 `activeSettingsTab: getInitialSettingsTab()`。
- 绑定 `hashchange` 事件，自动依据新哈希切换 Tab。

## 4. 验证与测试用例
1. **实时切换验证**:
   - 点击顶部主 Tab，观察 URL 是否切换。
   - 进入 `Settings`，点击各个子 Tab，观察 URL 是否变换成相应的 `#settings/xxxx`。
2. **状态持久与恢复验证**:
   - 直接在地址栏输入 `http://<host>:<port>/#settings/proxy` 并回车/刷新，验证是否可以直接打开并高亮展示代理设置面板。
3. **手动哈希变更验证**:
   - 处于首页时，手动修改 URL 哈希为 `#settings/compile`，验证页面是否瞬间切换至编译设置面板。
