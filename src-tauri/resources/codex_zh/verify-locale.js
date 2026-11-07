(async function () {
  var requested = __LOCALE_JSON__;
  var chinese = requested.toLowerCase().indexOf('zh') === 0;
  var zhMarkers = ['新建任务', '拉取请求', '已安排', '插件', '设置', '搜索'];
  var enMarkers = ['New task', 'Pull requests', 'Scheduled', 'Plugins', 'Settings', 'Search'];
  var count = function (text, markers) {
    return markers.reduce(function (n, marker) { return n + (text.indexOf(marker) >= 0 ? 1 : 0); }, 0);
  };
  var started = Date.now(), zhMatches = 0, enMatches = 0, ok = false;
  do {
    var text = document.body ? document.body.innerText || '' : '';
    zhMatches = count(text, zhMarkers);
    enMatches = count(text, enMarkers);
    ok = chinese ? zhMatches >= 2 && enMatches === 0 : enMatches >= 2 && zhMatches === 0;
    if (ok) break;
    await new Promise(function (resolve) { window.setTimeout(resolve, 200); });
  } while (Date.now() - started < 10000);
  var state = window.__codexZhI18nState || {};
  return JSON.stringify({ status: ok ? 'ok' : 'partial', requested: requested,
    documentLanguage: document.documentElement.lang || '', zhMarkers: zhMatches,
    enMarkers: enMatches, patchedClients: state.patchedClients || 0,
    patchedConfigs: state.patchedConfigs || 0 });
})()
