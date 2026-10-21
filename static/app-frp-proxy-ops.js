// FRP 角色/代理与上游代理操作薄封装模块。
// 由 app.js 拆出，在 app.js 之前以 <script defer> 加载，
// 通过共享全局作用域向 app.js 提供下列函数，无需修改调用方。
// 依赖的全局（proxyManager、frpManager）均为 app.js 顶层声明，加载顺序保证可用。

function loadProxyPresets() {
  return proxyManager.loadProxyPresets();
}

function renderProxyPresets() {
  return proxyManager.renderProxyPresets();
}

function loadAppProxyStatus() {
  return proxyManager.loadAppProxyStatus();
}

function clearActiveAppProxy() {
  return proxyManager.clearActiveAppProxy();
}

function clearProxyForm() {
  return proxyManager.clearProxyForm();
}

function saveProxyPreset() {
  return proxyManager.saveProxyPreset();
}

function showProxyResult(message, tone) {
  return proxyManager.showProxyResult(message, tone);
}

function syncProxyTestModeUi() {
  return proxyManager.syncProxyTestModeUi();
}

function testProxyFromForm() {
  return proxyManager.testProxyFromForm();
}

function setActiveFrpRoleTab(tab) {
  return frpManager.setActiveFrpRoleTab(tab);
}

function saveFrpRole(options) {
  return frpManager.saveFrpRole(options);
}

function setFrpRoleEditorVisible(visible) {
  return frpManager.setFrpRoleEditorVisible(visible);
}

function syncFrpCreateSourceModeUi(mode) {
  return frpManager.syncFrpCreateSourceModeUi(mode);
}

function loadFrpRoles() {
  return frpManager.loadFrpRoles();
}

function loadFrpSystemItems() {
  return frpManager.loadFrpSystemItems();
}

function setFrpCreateSourceMode(component, mode) {
  return frpManager.setFrpCreateSourceMode(component, mode);
}

function defaultFrpRole(component) {
  return frpManager.defaultFrpRole(component);
}

function fillFrpRoleForm(statusOrRole) {
  return frpManager.fillFrpRoleForm(statusOrRole);
}

function frpSystemItemById(id) {
  return frpManager.frpSystemItemById(id);
}

function testFrpPublicPort(host, port, statusEl) {
  return frpManager.testFrpPublicPort(host, port, statusEl);
}

function addFrpSourceToManaged() {
  return frpManager.addFrpSourceToManaged();
}

function addFrpServerSourceToManaged() {
  return frpManager.addFrpServerSourceToManaged();
}

function frpRoleStatusById(id) {
  return frpManager.frpRoleStatusById(id);
}

function testFrpRolePublicPort(id) {
  return frpManager.testFrpRolePublicPort(id);
}

function runFrpRoleCommand(command, pendingText, doneText, id) {
  return frpManager.runFrpRoleCommand(command, pendingText, doneText, id);
}

function unmanageFrpSystemItem(item) {
  return frpManager.unmanageFrpSystemItem(item);
}

function adoptFrpSystemItem(item) {
  return frpManager.adoptFrpSystemItem(item);
}

function syncFrpRoleComponentUi() {
  return frpManager.syncFrpRoleComponentUi();
}

function renderFrpProxyRows() {
  return frpManager.renderFrpProxyRows();
}

function fillFrpProxyEditor(proxy, index) {
  return frpManager.fillFrpProxyEditor(proxy, index);
}

function defaultFrpProxyConfig() {
  return frpManager.defaultFrpProxyConfig();
}

function editSelectedFrpProxy() {
  return frpManager.editSelectedFrpProxy();
}

function duplicateSelectedFrpProxy() {
  return frpManager.duplicateSelectedFrpProxy();
}

function deleteSelectedFrpProxies() {
  return frpManager.deleteSelectedFrpProxies();
}

function saveFrpProxyFromEditor() {
  return frpManager.saveFrpProxyFromEditor();
}

function setFrpProxyEditorVisible(visible) {
  return frpManager.setFrpProxyEditorVisible(visible);
}

function selectedFrpRoleStatus() {
  return frpManager.selectedFrpRoleStatus();
}

function downloadSelectedFrpRoleBinary() {
  return frpManager.downloadSelectedFrpRoleBinary();
}

function deleteSelectedFrpRole() {
  return frpManager.deleteSelectedFrpRole();
}

function syncFrpcProxyTypeUi() {
  return frpManager.syncFrpcProxyTypeUi();
}

function loadFrpcStatus() {
  return frpManager.loadFrpcStatus();
}

function downloadFrpcBinary() {
  return frpManager.downloadFrpcBinary();
}

function saveFrpcConfig(options) {
  return frpManager.saveFrpcConfig(options);
}

function runFrpcCommand(command, pendingText, doneText) {
  return frpManager.runFrpcCommand(command, pendingText, doneText);
}

function loadFrpsStatus() {
  return frpManager.loadFrpsStatus();
}

function downloadFrpsBinary() {
  return frpManager.downloadFrpsBinary();
}

function saveFrpsConfig(options) {
  return frpManager.saveFrpsConfig(options);
}

function runFrpsCommand(command, pendingText, doneText) {
  return frpManager.runFrpsCommand(command, pendingText, doneText);
}
