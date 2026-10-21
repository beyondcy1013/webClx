# webClx launch and revenue copy

This file is an execution pack, not evidence that every channel has already
been posted. Use the canonical product page as the destination:

- Product: https://beyondcy1013.github.io/webClx/
- Source: https://github.com/beyondcy1013/webClx
- Release: https://github.com/beyondcy1013/webClx/releases/tag/v1.8.11
- Trial waitlist: https://github.com/beyondcy1013/webClx/issues/new?template=hosted-trial.yml

The hosted preview is invite-only. Never describe it as instant, generally
available, or production SaaS until customer DNS/TLS, backup recovery, disk
quotas, legal identity, and payment operations are verified.

## Positioning

Do not lead with "multi-agent orchestration." Lead with the expensive moment:
a developer leaves the desk while a build, deployment, or coding Agent still
needs attention.

Primary promise, updated from public issue evidence collected on 2026-08-14:

> Monitor, approve, or steer the same live CLI from your phone. Return to the
> same live session at your desk.

Supporting proof:

- native Codex, Claude, DeepSeek Harness, and shell sessions remain intact;
- tmux-backed terminals survive browser closure and device changes;
- the bundled Skill hands work to another Harness for read-only review;
- self-hosted users keep code, workspaces, and model configuration on their own
  infrastructure.

Do not position the phone as the primary coding surface. Public requests in
Codex, Claude Code, Command Code, Freebuff, mux-pod, and other projects
repeatedly describe the phone as an oversight surface for progress, approvals,
interrupts, and short steering while the workstation remains primary. The
strongest differentiator is the same tmux-backed live CLI plus cross-Harness
handoff, not generic "AI coding on mobile."

Revenue offers:

1. Free AGPL self-hosting builds trust and adoption.
2. Deployment support starts at USD 49 for people who own a server but do not
   want to configure installation, TLS, network controls, upgrades, and a
   recovery exercise.
3. Invite-only managed personal preview starts at USD 8/month, excluding cloud
   infrastructure and model usage.
4. Professional hosting and commercial licensing remain manually scoped.

## Channel status

| Channel | Status now | Publish condition | Objective |
| --- | --- | --- | --- |
| GitHub Discussions | Ready | Product page and Release live | Recruit technical trial users |
| Awesome Claude Code (jqueryscript list) | Submitted | Maintainer review of issue #591 | Reach Claude Code users looking for mobile/remote clients |
| DEV Community / Hashnode | Draft ready | Owner account | Search traffic and self-host installs |
| Indie Hackers | Draft ready | Owner account | Pricing and problem interviews |
| Reddit | Draft ready, adapt per subreddit | Read current rules and participate from owner account | Technical feedback |
| Show HN | Ready for owner submission | Interactive synthetic demo works without signup or email | Engineering feedback |
| Product Hunt | Hold | 60-90 second real video and 3-5 real trial outcomes | Product launch |
| AlternativeTo | Hold | Stable product page, support identity, and durable service status | Long-tail discovery |
| Awesome Selfhosted | Not eligible before 2026-12-14 | First public release at least four months old | Directory discovery |

Do not mechanically cross-post on the same day. Publish one technical article,
answer every substantive comment, record objections, then adapt the next post.

### Awesome Claude Code submission (manual Web UI only)

The repository's contribution rules explicitly prohibit CLI/API submissions and
require a human to use the issue form. Do not automate this step. The project
meets the age/active-development gate; submit only after personally reviewing
the list and agreeing with its Code of Conduct.

- Form: https://github.com/hesreallyhim/awesome-claude-code/issues/new?template=recommend-resource.yml
- Display name: `webClx`
- Category: `Remote Control, Notifications & Voice I/O`
- Link: `https://github.com/beyondcy1013/webClx`
- Author name: `beyondcy1013`
- Author link: `https://github.com/beyondcy1013`
- Description: `A self-hosted browser workspace that keeps native Claude Code sessions live in tmux across desktop and mobile browsers. It includes a terminal messaging Skill for handing a task to Codex or DeepSeek Harness for read-only review while the original session remains the writer.`

The description is deliberately factual and does not promise hosted availability,
automatic discovery of every external session, or phone-first coding.

The compatible open-submission directory also has a live recommendation issue:
https://github.com/jqueryscript/awesome-claude-code/issues/591

## Tracking links

- DEV:
  `https://beyondcy1013.github.io/webClx/?utm_source=devto&utm_medium=article&utm_campaign=public-preview`
- Hashnode:
  `https://beyondcy1013.github.io/webClx/?utm_source=hashnode&utm_medium=article&utm_campaign=public-preview`
- Indie Hackers:
  `https://beyondcy1013.github.io/webClx/?utm_source=indiehackers&utm_medium=community&utm_campaign=pricing-validation`
- Reddit:
  `https://beyondcy1013.github.io/webClx/?utm_source=reddit&utm_medium=community&utm_campaign=public-preview`
- Hacker News, when eligible:
  `https://beyondcy1013.github.io/webClx/?utm_source=hackernews&utm_medium=community&utm_campaign=show-hn`
- Product Hunt, when eligible:
  `https://beyondcy1013.github.io/webClx/?utm_source=producthunt&utm_medium=launch&utm_campaign=product-hunt`

The GitHub Issue Form does not provide a reliable analytics backend by itself.
Until a privacy-reviewed application form exists, ask applicants for their
discovery source and record it manually without collecting model credentials.

## DEV / Hashnode article

### Title

I stopped rebuilding coding Agents for mobile and kept their native terminals
instead

### Body

The realistic phone workflow is not writing an application on a touch keyboard.
It is checking whether a long task is stuck, approving or interrupting an
action, sending a short correction, and returning to the same live CLI later.
Many mobile interfaces instead create a smaller second chat UI, another
context, and another place where long-running work can disappear.

I built webClx around the opposite choice: keep the native Codex, Claude,
DeepSeek Harness, and shell terminals alive with tmux, then make those sessions
available in a browser workspace that also works from a phone.

The workflow I care about is ordinary but expensive when it breaks:

1. start a coding or deployment task at a workstation;
2. leave the desk without terminating the terminal;
3. check build output or logs from a phone;
4. ask another Harness for a read-only review through the bundled terminal
   messaging Skill;
5. resume the same native context later.

webClx is written in Rust, is AGPL-3.0-or-later, and is currently a developer
preview. It has administrative access to files and terminals, so remote use
requires TLS and network controls; the management port should never be exposed
directly.

The source and versioned v1.8.11 archive are public. I am also inviting a small
number of users to a seven-day isolated hosted preview while I validate setup,
support time, and pricing. Each approved trial is intended to use a separate OS
user, service, workspace, credentials, and source port boundary. It is not a
shared administrator account.

Product and synthetic workflow demonstration:
https://beyondcy1013.github.io/webClx/?utm_source=devto&utm_medium=article&utm_campaign=public-preview

Source: https://github.com/beyondcy1013/webClx

I am looking for concrete feedback: when you leave your workstation, which
coding task is most painful to monitor or continue from a phone?

## Indie Hackers post

### Title

Validating an $8/month managed version of an open-source mobile coding workspace

### Body

I built webClx for a problem I repeatedly had myself: a coding Agent or build is
still running when I need to leave the workstation. Phone SSH works, but it is
awkward for switching persistent sessions, browsing the workspace, monitoring
builds, and handing a task to another Harness for review.

The open-source product keeps native Codex, Claude, DeepSeek Harness, and shell
terminals persistent behind a browser UI. The monetization hypothesis is not
"charge for more AI." It is charge for isolated hosting and the operational
work around TLS, upgrades, backup targets, recovery, and support.

Current preview offers:

- AGPL self-hosting: free;
- deployment support on the user's server: from $49;
- invite-only isolated personal hosting: $8/month preview, infrastructure and
  model usage separate.

I am deliberately not enabling unattended billing yet. I first need to measure
activation, support time, seven-day retention, and the reasons people refuse to
pay.

Product page:
https://beyondcy1013.github.io/webClx/?utm_source=indiehackers&utm_medium=community&utm_campaign=pricing-validation

Question for builders who use coding Agents: would you pay for managed remote
access, or is a one-time deployment service the more valuable offer?

## Reddit adaptation

Use only in a subreddit whose current rules allow project posts. Remove pricing
from the title, disclose that you are the author, and spend most of the post on
the technical decision.

### Title

I made a self-hosted browser workspace that keeps native coding-agent terminals
persistent on mobile

### Body

I am the author of webClx. Instead of recreating Codex/Claude/DeepSeek Harness
inside another chat UI, it keeps their native tmux-backed terminals and exposes
the same sessions through a desktop or mobile browser.

The part I am testing now is cross-Harness handoff: one terminal remains the
writer, while another receives a read-only review request through a bundled
Skill and replies to the original session.

It is AGPL and intended for trusted self-hosted infrastructure behind TLS and
network controls. I would appreciate criticism of the security boundary and
mobile workflow, especially from people already using persistent SSH/tmux
setups.

Demo and source:
https://beyondcy1013.github.io/webClx/?utm_source=reddit&utm_medium=community&utm_campaign=public-preview

## Show HN draft

### Title

Show HN: webClx - continue native Codex, Claude, and DeepSeek terminals from a phone

### Body

I built webClx because I wanted to monitor and steer coding-agent work after
leaving my desk, then return to the same live CLI without replacing the
Agent's native terminal with another chat history.

It keeps Codex, Claude, DeepSeek Harness, and shell sessions alive with tmux and
makes the same workspace available in a desktop or mobile browser. A bundled
terminal messaging Skill can send a task to another Harness for read-only
review and route the reply back while one terminal remains the writer.

You can try the complete desktop-to-phone and cross-Harness workflow without an
account or email here:

https://beyondcy1013.github.io/webClx/demo.html?utm_source=hackernews&utm_medium=community&utm_campaign=show-hn

The demo is deliberately browser-only and synthetic: it does not connect to a
server or contain customer data. The real project is AGPL-3.0-or-later and
self-hosted. Because it controls files and terminals, remote deployments need
TLS and network controls rather than an exposed management port.

Source: https://github.com/beyondcy1013/webClx

I would particularly value criticism from people using SSH/tmux or coding
Agents remotely: which part of this workflow is useful, and which part is still
worse than your current setup?

## Product Hunt assets - do not publish yet

- Tagline: `Continue your coding-agent terminals from any browser or phone`
- Short description: `Self-hosted persistent Codex, Claude, DeepSeek Harness,
  and shell sessions with mobile access and cross-Harness task handoff.`
- Required media before launch: a real 60-90 second desktop-to-phone recording,
  three product screenshots, and no internal paths or credentials.
- Required proof before launch: 3-5 real trial outcomes, support contact,
  privacy/deletion contact, and a working application path.

## Chinese short copy

webClx 是 Codex、Claude、DeepSeek Harness 与 Shell 的自托管浏览器工作区。
它保留原生终端和上下文，让长任务在关闭浏览器后继续运行，并可从手机恢复、查看
构建与日志、向另一 Harness 发起只读 review。AGPL 自托管免费；7 天隔离托管试用
目前采用人工审核候补，不共享管理员账号。产品页：
https://beyondcy1013.github.io/webClx/

## Weekly operating loop

Track these numbers once per week:

```text
channel -> meaningful conversations -> trial applications -> approved trials
-> first mobile terminal success -> active on 3 of 7 days -> paid / declined
support minutes -> infrastructure cost -> contribution margin
```

Optimize for conversations, activation, and payment evidence. Do not optimize
for impressions, stars, or raw download counts in isolation.
