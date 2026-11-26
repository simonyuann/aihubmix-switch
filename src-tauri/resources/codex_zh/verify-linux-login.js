// The Linux desktop shows an authentication screen before its workspace shell.
// Only fixed visible actions count. Chat content and editable text are excluded.
(function () {
  const visible = (element) => !element.closest(
    'article, [role="log"], [contenteditable="true"], [aria-hidden="true"], [hidden]'
  ) && element.getClientRects().length > 0;
  const loginScreen = Array.from(document.querySelectorAll('h1, [role="heading"]'))
    .some((heading) => visible(heading) && heading.innerText?.trim() === '登录 ChatGPT');
  if (!loginScreen) return false;
  const labels = new Set(['继续登录', '使用其他方式登录', '注册']);
  const found = new Set();
  for (const control of document.querySelectorAll('button, a, [role="button"], [role="link"]')) {
    if (!visible(control)) continue;
    const name = [control.innerText, control.getAttribute('aria-label'), control.getAttribute('title')]
      .find((value) => typeof value === 'string' && labels.has(value.trim()));
    if (name) found.add(name.trim());
  }
  return found.has('继续登录') && (found.has('使用其他方式登录') || found.has('注册'));
})()
