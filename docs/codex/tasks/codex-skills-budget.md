# Codex Skills 2% Context Budget

## Symptom

Codex shows a startup notice such as:

> Skill descriptions were shortened to fit the 2% skills context budget. Codex can still see every skill, but some descriptions are shorter.

The full system prompt auto-truncates each `description:` line in `SKILL.md` frontmatter, but the truncated version still does not always fit. Long descriptions in active skills drive the warning.

## Scope

- Skill roots: `r0 = /home/root/.codex/skills`, `r1 = /home/root/.codex/skills/.system`.
- The disabled-skills allowlist already lives in `~/.codex/config.toml` under `[[skills.config]] enabled = false`. Add paths there to drop a skill entirely.
- Trimming the `description:` field in the `SKILL.md` frontmatter is the second lever; it preserves the skill and its trigger info while shrinking the system prompt's footprint.

## Verified Approach (2026-06-02)

1. Walked the active skill list (the entries shown in the Codex system prompt "Available skills" section).
2. Parsed the YAML frontmatter of each `SKILL.md` and replaced `description:` with a trimmed version (target <= 110 chars, preserving any `Use when ...` clause when present).
3. Special name/path mappings that differ from the directory name:
   - `design-taste-frontend` -> `/home/root/.codex/skills/taste-skill/SKILL.md`
   - `executing-plans` / `systematic-debugging` / `test-driven-development` / `using-superpowers` / `verification-before-completion` / `writing-plans` -> `superpowers-*` directories.
4. Result for the 77 Codex skills visible in the system prompt: total description length dropped from ~14k chars to ~6.5k chars, well under the 2% of a 200k context window (16k chars / 4k tokens).

## Verified Cleanup (2026-07-22)

The warning returned after the global catalog grew through new skills, project-skill symlinks, and backup entries.

Baseline from `webClx`:

- 149 active catalog entries and 146 canonical targets.
- 25,195 raw description characters, including six system-managed skills.
- 94 user-managed descriptions over 100 characters.
- 41 active global symlink entries, three `.bak.*` entries, four duplicate names, and 56 stale disabled paths.

Final verified state:

- 122 active catalog entries and 120 canonical targets.
- 11,623 raw description characters and zero user-managed descriptions over 100 characters.
- 17 active global symlinks, zero scanned backups, zero duplicate names, zero stale disabled paths, and zero malformed frontmatter.
- Fresh ephemeral `codex exec` startup returned successfully with no skills-budget warning.
- Equivalent audits passed from `/home/codes/lyyStock/stockScreener` (125 skills, 12,108 description characters) and `/home/codes/apps/kidsAI` (117 skills, 11,372 description characters).

Use the checked audit instead of ad hoc counts:

```bash
bash tests/audit-codex-skills-budget.test.sh
bash scripts/audit-codex-skills-budget.sh \
  --config /home/root/.codex/config.toml \
  --root /home/root/.codex/skills \
  --root /home/codes/tools/webClx/.codex/skills \
  --max-description-chars 13000 \
  --max-active-skills 120
```

The reversible archive is `/home/root/.codex/skills-archive/2026-07-22/`. It contains the original descriptions, moved backup entries, archived project aliases, sync-generated copies, and path manifests. The pre-cleanup config backup is `/home/root/.codex/config.toml.skills-budget-20260722.bak`.

Current policy:

1. Keep user-managed `description:` values at no more than 100 characters and preserve distinctive `Use when ...` triggers.
2. Leave `.system` descriptions untouched; count them in the total budget.
3. Keep project-specific skills under the owning project's `.codex/skills`. Add a global symlink only when the skill must route requests from unrelated working directories.
4. Keep active descriptions below 13,000 characters and retain at least modest headroom rather than targeting the exact warning boundary.
5. Run the audit after adding or globally linking a skill.

## Cautions

- Trimming loses trigger words; when adding a brand new skill, keep the description at or below 100 chars from the start.
- `superpowers-*` skills use unquoted YAML `description:` lines; the script handled both quoted and unquoted forms.
- The `/home/beyondcy/.claude/skills/` root is also scanned by Codex but is not part of the displayed "Available skills" list; trimming it is optional.
- The blacklist in `~/.codex/config.toml` is the only reliable way to remove a skill from the system prompt entirely. Trimming is cheaper than blacklisting if the skill is still useful.
- `skill-bidirectional-sync` atomically replaces targets and can turn a Codex skill-directory symlink into a copied directory while leaving the original symlink as `.bak.<timestamp>`. After syncing a globally linked project skill, restore the canonical symlink and move the generated copy/backup outside `/home/root/.codex/skills` before auditing.

## 离线库与跨 Agent 技能发现（2026-10-16）

离线存储、安装目录和当前会话已加载列表是三件事。离线库并不会自动进入所有 Agent 的上下文。

2026-10-17：通用的技能库管理与发现排查经验已提炼至用户技能 `skill-organizer/references/skill-discovery.md`，由其 `SKILL.md` 按需路由。检索仍归 `offline-skill-locator`，安装与同步仍归 `skill-bidirectional-sync`；本专题保留当次审计证据，不复制通用流程。

| 用途 | 本机位置 | 说明 |
| --- | --- | --- |
| 全量离线库 | `/home/codes/offlineSkills` | `find-skill <关键词>` 检索；`--show <名称>` 读取 |
| Codex 用户技能 | `/home/root/.codex/skills` | 同步技能的当前维护源在此 |
| Claude 用户技能 | `/home/root/.claude/skills` | 含指向 Codex 的共享软链接 |
| agy / Antigravity CLI | `/home/root/.gemini/config/skills` | 本机 agy 内置发现说明确认的全局目录 |
| Gemini CLI | `/home/root/.gemini/skills` | 独立于 agy；用同步器 `--gemini-root` 指定 |

root 的 passwd HOME 与本次执行环境 HOME 均为 `/home/root`，不能凭用户名猜成 `/root`。上表是这台机器的路径，不是所有用户的固定路径。

本次清点：离线索引 625 项、17 个分类键，包含 230 项 backups，非备份 395 项；旧文档所写“16 类”未包含后来增加的 misc。修复前用户技能目录的顶层可读 `SKILL.md` 计数为 Codex 37、Claude 31、agy 32、Gemini CLI 2；这不是 Agent 实际加载数量，也不包含项目和系统技能。

已确认的同步缺陷及修复：

- 离线同步技能落后于 Codex 维护源，现已更新文档、脚本及检索描述。
- `sync-gemini` 原来只扫描 Claude，而扫描器跳过目录软链接。Claude 中的同步技能正是软链接，因此出现文件存在但同步来源显示缺失。改为 Codex 优先、Claude 兜底。
- `--with-gemini` 原来依赖 Codex/Claude 本轮发生变化，并读取同步前的快照。改为始终执行 Gemini fan-out，并重新扫描来源，补齐目标缺失和刚新增的技能。
- agy 原有两个入口均为可读软链接；本次换为真实技能目录以减少跨工具扫描差异。尚无证据证明 agy 不支持软链接，不能将其认定为本次会话找不到技能的根因。
- Gemini CLI 目录原来缺少 `skill-bidirectional-sync` 与 `offline-skill-locator`，现已补齐。

安装只处理这两个入口，没有把 395 项全量激活，也未开启后台 watcher。同步技能变更已按 dry-run 后明确选择 Codex 源，同步到 Claude、Cursor、Devin、ZCode、WorkBuddy、Kiro、Grok 及 agy/Gemini；Kiro/Grok 的 workspace 目标显式指向各自全局目录，未向 webClx 工程增加技能副本。

使用：

```bash
find-skill skill-bidirectional-sync
find-skill --show skill-bidirectional-sync
python3 /home/root/.codex/skills/skill-bidirectional-sync/scripts/skill_bidirectional_sync.py sync-gemini --dry-run <技能名>
python3 /home/root/.codex/skills/skill-bidirectional-sync/scripts/skill_bidirectional_sync.py sync-gemini <技能名>
# Gemini CLI 独立安装目标，先 dry-run，再去掉 --dry-run 执行
python3 /home/root/.codex/skills/skill-bidirectional-sync/scripts/skill_bidirectional_sync.py --gemini-root /home/root/.gemini/skills sync-gemini --dry-run <技能名>
```

`find-skill --activate` 当前仍固定安装到 `/home/codes/.agents/skills`，不是“自动识别当前 Agent”，也不能让当前对话立即刷新。需要 agy/Gemini 时使用上述显式安装目标；当前对话可直接读取 `--show` 内容，新会话再检查发现列表。

验证：同步技能 frontmatter 校验通过；4 项临时目录回归覆盖 Claude 软链接、两端相同但 Gemini 缺失、新增 Claude 副本、Claude-only 兜底，并确认 dry-run 不写目标。两个 Gemini 目标的入口文件与 Codex 源内容一致。未启动真实模型会话验证发现列表；若新开 agy 后仍缺失，应继续检查该会话 HOME、发现配置与加载列表，不将文件存在等同于运行验收。
