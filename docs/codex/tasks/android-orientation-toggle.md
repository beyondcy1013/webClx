# Android Orientation Toggle

## 2026-08-15

- Android WebView 默认锁定竖屏，避免平板/手机系统方向把终端页意外切到横屏。
- 底部悬浮菜单新增方向按钮，仅在本机 bridge 暴露 `getScreenOrientation` /
  `setScreenOrientation` 时显示；普通浏览器隐藏。
- 点击顺序为 `portrait -> landscape -> system -> portrait`，状态持久化在
  `app_settings/display.screen_orientation`，应用重启后由原生侧恢复。
- 方向常量与持久化在 `AppPreferences`；实际 `setRequestedOrientation()` 与 WebView
  bridge 在 `MainActivity`；FAB 状态和循环切换在 `terminal.html` / `terminal.js`。
- 验证：`node --test tests/android-client-contract.test.mjs`、`node --check static/terminal.js`、
  Android release 构建与 v2/v3 验签通过。
- `webClx-1.8.11.apk`（versionCode `1008011`，SHA-256
  `d6c01e7daeb9e4fcd54a97e9f2aa50c5bc64a0b9e20827b13fc5e2bfdddd3e99`）已发布到下载中心，
  并同步部署运行目录 `static/`。

移动端真机还需确认：按钮在 Android WebView 可见、三态切换立即生效、重启后保持选择、
普通浏览器不显示该按钮。

## 2026-08-16

- 右侧悬浮菜单不再提供单击三态循环；该操作会立即锁定横屏并持久化，误触后缺少独立恢复入口。
- 快捷按钮改为单向“恢复竖屏”：竖屏时隐藏，仅在原生 bridge 报告横屏或跟随系统时显示，点击固定写入 `portrait`。
- 该修复仅修改权威静态目录，Android 客户端重新加载终端页即可获得，无需安装新 APK。
