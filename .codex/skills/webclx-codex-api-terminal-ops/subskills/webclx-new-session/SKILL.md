---
name: webclx-new-session
description: webClx 新开终端与指定预设对话框规范，覆盖新会话、恢复、Fork、提问子Tab及Agent分支切换时的统一高度锁定、防闪烁稳定滚动槽与控件一致性。
---

# webClx 新开对话框与会话动作子 Tab 规范 (webclx-new-session)

## 适用范围

本技能专用于维护 webClx 终端管理中的“新开终端 / 指定预设执行”模态对话框（`#terminal-specified-task-dialog`）。
涵盖会话动作（新会话 `new`、恢复 `resume`、Fork `fork`、提问 `ask`）及不同 Agent 分支（Codex、Claude、AGY、CodeBuddy 等）切换时的布局、几何稳定性与参数组装规范。

## 核心设计规范与防闪烁约束

### 1. 对话框统一高度锁定（杜绝窗体上下跳动）

- **统一容器高度**：
  在 `#terminal-specified-task-dialog` 声明恒定高度：
  ```css
  #terminal-specified-task-dialog {
    height: min(640px, calc(100dvh - 32px));
    max-height: calc(100dvh - 32px);
  }
  ```
- **禁止 `height: auto`**：
  HTML 原生 `<dialog>` 默认使用 `margin: auto` 在视口中垂直居中。如果高度随内容动态改变，切换动作或 Agent 时整个弹窗会在屏幕上剧烈跳动（如由 350px 突增至 550px 会使弹窗瞬间下沉或上移上百像素）。
- **全高 Flex 表单容器**：
  `#terminal-specified-task-form` 必须为 `display: flex; flex-direction: column; height: 100%; box-sizing: border-box;`。

### 2. 上方通用控件保留与防压缩（保持 Tab 结构一致）

- **工作目录在所有子 Tab 统一保留**：
  `#terminal-specified-task-workspace-field`（工作目录选择与浏览）必须在所有子 Tab（新会话、恢复、Fork、提问）中永久保留显示与操作能力，严禁仅在新会话显示而在其他 Tab 隐藏。
  - 逻辑所有者：`static/terminal-specified-task.js` 中的 `syncTerminalSpecifiedTaskForm()`：
    ```javascript
    const workspaceSelectable = !terminalSpecifiedTaskId;
    if (dom.workspaceField) {
      dom.workspaceField.hidden = false;
    }
    ```
- **上方控件防压缩**：
  `#terminal-specified-task-form > *` 与 `#terminal-specified-task-form .terminal-specified-task-fixed-options > *` 统一声明 `flex-shrink: 0;`。
  顶部标题、工作目录、Agent 单选栏、账号预设选择栏、执行方式、会话动作组等永远保持自然高度，绝不在视口偏小时被压扁变形。

### 3. 稳定滚动槽 `scrollbar-gutter: stable;`（杜绝控件宽度缩放闪烁）

- **根因分析**：
  “新会话”控件通常多于“恢复”或“Fork”。当新会话微量溢出出现垂直滚动条时，会占用约 15px 的可用宽度；切换到无滚动条的子 Tab 时，可用宽度瞬间增加 15px。若未固定滚动槽，所有百分比或 `1fr` 宽度的输入框、下拉框、单选按钮和文本框在 Tab 切换时会突兀地横向变宽或缩窄 15px，产生强烈的视觉抖动闪烁。
- **强制规则**：
  ```css
  #terminal-specified-task-form {
    overflow-y: auto;
    scrollbar-gutter: stable;
  }
  ```
  `scrollbar-gutter: stable;` 永久保留滚动条轨道空间，确保 Tab 切换时容器可用宽度绝对不变。

### 4. 复合控件单行化与防多行撑高

- **群组选项 4 列单行化**：
  “加入当前群组”包含复选框、文字“加入当前群组”、群组下拉框、角色下拉框共 4 个子元素。网格必须声明 4 个列轨道：
  ```css
  .terminal-specified-task-group-option {
    display: grid;
    grid-template-columns: auto auto minmax(130px, 1fr) minmax(100px, 0.7fr);
    align-items: center;
    gap: 8px;
  }
  ```
  严禁缺少第 4 列导致角色下拉框折行到第二行，从而凭空增加 40px 高度。

### 5. 提示词区域自适应吸收高度差异

- **自适应剩余空间**：
  提示词区域声明：
  ```css
  #terminal-specified-task-form .terminal-specified-task-prompt-field {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    align-items: stretch;
    flex: 1 1 auto;
    min-height: 60px;
  }
  #terminal-specified-task-form .terminal-specified-task-prompt-labels {
    width: 84px;
    flex-shrink: 0;
  }
  #terminal-specified-task-form .terminal-specified-task-text {
    height: 100%;
    min-height: 52px;
    box-sizing: border-box;
    resize: vertical;
  }
  ```
- **底部操作栏吸底锚定**：
  `.dialog-actions` 设置 `margin-top: auto; flex-shrink: 0; padding-top: 4px;`，关闭与启动按钮始终稳固锚定在弹窗底边。

### 6. AGY 默认恢复最后一次会话（`-c`）联动

- 当选择 AGY 且会话动作为“恢复”时，展示“默认最后一次会话”复选框（`#terminal-specified-task-agy-continue`）。
- 勾选时：
  - `#terminal-specified-task-session-id` 输入框置灰 (`disabled = true`)，占位符更新为 `默认恢复最后一次会话 (-c)`。
  - 启动参数使用 `agy -c`。
- 未勾选时：
  - 输入框激活且 `required = true`，启动参数使用 `agy --conversation <session_id>`。

## 回归验证清单

1. 切换会话动作（新会话 -> 恢复 -> Fork -> 提问）：
   - 弹窗外框尺寸与屏幕居中位置完全不发生任何像素偏移。
   - 上方“工作目录”控件在每个 Tab 均完整显示，下拉框与浏览按钮功能正常。
   - 所有横向输入框、下拉框、单选按钮和提示词框宽度保持 100% 恒定，无滚动条进出引起的宽度缩放拉伸。
2. 切换 Agent 分支（Codex -> Claude -> AGY -> CodeBuddy 等）：
   - 布局保持平滑稳定。
   - 选定 AGY + 恢复时，“默认最后一次会话”多选框正常显隐，勾选联动置灰有效。
3. 执行自动化测试：
   ```bash
   node tests/terminal-specified-task-unified-height.test.mjs
   node tests/agy-terminal-resume-continue.test.mjs
   node tests/terminal-agent-commands.test.mjs
   ```
