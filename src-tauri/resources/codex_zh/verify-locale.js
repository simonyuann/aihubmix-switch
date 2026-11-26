(async function () {
  var requested = __LOCALE_JSON__;
  var chinese = requested.toLowerCase().indexOf('zh') === 0;
  // Inspect shell controls only. Chat messages and workspace names routinely
  // contain labels such as "Settings" even when the entire interface is Chinese.
  var labels = chinese
    ? ['新建任务', '新建对话', '新对话', '新聊天', '拉取请求', '已安排', '插件', '设置', '搜索', '技能', '自动化']
    : ['New task', 'New chat', 'Pull requests', 'Scheduled', 'Plugins', 'Settings', 'Search', 'Skills', 'Automations'];
  var started = Date.now(), matches = 0, ok = false;
  do {
    var found = new Set();
    var controls = document.querySelectorAll('button, a, [role="button"], [role="menuitem"], [role="tab"], [role="link"]');
    for (var item of controls) {
      if (item.closest('article, [role="log"], [contenteditable="true"], [aria-hidden="true"], [hidden]') || !item.getClientRects().length) continue;
      var names = [item.getAttribute('aria-label'), item.getAttribute('title'), item.innerText];
      for (var name of names) {
        if (typeof name === 'string' && labels.includes(name.trim())) found.add(name.trim());
      }
    }
    matches = found.size;
    ok = matches >= 2;
    if (ok) break;
    await new Promise(function (resolve) { window.setTimeout(resolve, 200); });
  } while (Date.now() - started < 10000);
  return JSON.stringify({ status: ok ? 'ok' : 'partial', requested: requested, matchedControls: matches });
})()
