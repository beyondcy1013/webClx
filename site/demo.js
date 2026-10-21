const demo = {
  step: 1,
  view: 'desktop',
};

const output = document.querySelector('.terminal-output');
const phoneMessage = document.querySelector('.phone-message');
const connectionState = document.querySelector('.connection-state');
const complete = document.querySelector('.demo-complete');
const actionButtons = Object.fromEntries(
  [...document.querySelectorAll('[data-action]')].map((button) => [button.dataset.action, button]),
);

function language() {
  return document.documentElement.lang.startsWith('zh') ? 'zh' : 'en';
}

function text(en, zh) {
  return language() === 'zh' ? zh : en;
}

function appendEvent(kind, en, zh) {
  const event = document.createElement('div');
  event.className = `terminal-event ${kind}`;
  event.textContent = text(en, zh);
  output.append(event);
}

function setView(view) {
  demo.view = view;
  const workspace = document.querySelector('.demo-workspace');
  workspace.classList.toggle('view-desktop', view === 'desktop');
  workspace.classList.toggle('view-mobile', view === 'mobile');
  document.querySelectorAll('[data-view]').forEach((button) => {
    button.setAttribute('aria-selected', String(button.dataset.view === view));
  });
}

function renderProgress() {
  document.querySelector('.progress-fill').style.width = `${((demo.step - 1) / 4) * 100}%`;
  document.querySelectorAll('.demo-steps article').forEach((item) => {
    const step = Number(item.dataset.step);
    item.classList.toggle('done', step < demo.step);
    item.classList.toggle('current', step === demo.step);
  });
}

function advance(action) {
  if (action === 'disconnect' && demo.step === 1) {
    appendEvent('disconnect', 'Desktop browser closed. The tmux session remains active.', '桌面浏览器已关闭，tmux 会话仍在后台运行。');
    connectionState.textContent = text('DESKTOP AWAY · SESSION ACTIVE', '桌面离开 · 会话仍在运行');
    connectionState.dataset.state = 'away';
    actionButtons.disconnect.disabled = true;
    actionButtons.resume.disabled = false;
    actionButtons.resume.classList.add('primary-action');
    actionButtons.disconnect.classList.remove('primary-action');
    demo.step = 2;
  } else if (action === 'resume' && demo.step === 2) {
    setView('mobile');
    appendEvent('resume', 'Mobile browser resumed feature-implement with the same terminal history.', '手机浏览器已恢复 feature-implement，并保留原终端历史。');
    connectionState.textContent = text('PHONE CONNECTED · SAME SESSION', '手机已连接 · 同一会话');
    connectionState.dataset.state = 'phone';
    actionButtons.resume.disabled = true;
    actionButtons.review.disabled = false;
    actionButtons.review.classList.add('primary-action');
    actionButtons.resume.classList.remove('primary-action');
    demo.step = 3;
  } else if (action === 'review' && demo.step === 3) {
    setView('desktop');
    document.querySelector('[data-session="review"]').classList.add('selected');
    appendEvent('review', 'Delivered to security-review: review the diff read-only and focus on auth boundaries.', '已投递到 security-review：只读复核当前 diff，重点检查认证边界。');
    phoneMessage.hidden = false;
    phoneMessage.textContent = text('Review request delivered to Claude.', '复核请求已投递给 Claude。');
    actionButtons.review.disabled = true;
    actionButtons.reply.disabled = false;
    actionButtons.reply.classList.add('primary-action');
    actionButtons.review.classList.remove('primary-action');
    demo.step = 4;
  } else if (action === 'reply' && demo.step === 4) {
    appendEvent('reply', 'Claude replied: no credential leakage found; add one rate-limit edge-case test.', 'Claude 已回复：未发现凭据泄露；建议补充一个限流边界测试。');
    phoneMessage.textContent = text('Claude replied: add one rate-limit edge-case test.', 'Claude 已回复：补充一个限流边界测试。');
    actionButtons.reply.disabled = true;
    actionButtons.reply.classList.remove('primary-action');
    demo.step = 5;
    complete.hidden = false;
    complete.scrollIntoView({ behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth', block: 'center' });
  }
  renderProgress();
}

function resetDemo() {
  location.reload();
}

document.querySelectorAll('[data-action]').forEach((button) => {
  button.addEventListener('click', () => advance(button.dataset.action));
});
document.querySelectorAll('[data-view]').forEach((button) => {
  button.addEventListener('click', () => setView(button.dataset.view));
});
document.querySelectorAll('.session').forEach((button) => {
  button.addEventListener('click', () => {
    document.querySelectorAll('.session').forEach((item) => item.classList.remove('selected'));
    button.classList.add('selected');
  });
});
document.querySelector('.reset-demo').addEventListener('click', resetDemo);
renderProgress();
