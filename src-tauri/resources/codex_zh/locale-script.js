(async function () {
  var locale = __LOCALE_JSON__;
  // Current Codex exposes its settings service through the shared renderer module.
  // Older releases have no matching preload and continue through the legacy bridge.
  var sharedLink = document.querySelector('link[rel="modulepreload"][href*="app-shared-"]');
  if (sharedLink && sharedLink.href) {
    try {
      var module = await import(sharedLink.href);
      var deadline = Date.now() + 8000;
      while ((!module.Bct || !module.Bct.settings) && Date.now() < deadline) {
        await new Promise(function (resolve) { window.setTimeout(resolve, 100); });
      }
      var settings = module.Bct && module.Bct.settings;
      if (!settings || typeof settings.write !== 'function' || typeof settings.read !== 'function') {
        return JSON.stringify({ status: 'partial', reason: 'settings-service-unavailable', locale: locale });
      }
      await settings.write('localeOverride', locale);
      var current = await settings.read('localeOverride');
      var applied = current && current.effective === locale;
      if (applied) window.setTimeout(function () { window.location.reload(); }, 600);
      return JSON.stringify({ status: applied ? 'ok' : 'partial', locale: locale, effective: current && current.effective, method: 'settings-service' });
    } catch (error) {
      return JSON.stringify({ status: 'partial', reason: 'settings-service-error', locale: locale, error: String(error) });
    }
  }
  var started = Date.now();
  while ((!window.electronBridge || typeof window.electronBridge.sendMessageFromView !== 'function') && Date.now() - started < 8000) {
    await new Promise(function (resolve) { window.setTimeout(resolve, 100); });
  }
  var bridge = window.electronBridge;
  if (!bridge || typeof bridge.sendMessageFromView !== 'function') {
    return JSON.stringify({ status: 'partial', reason: 'official-setting-bridge-unavailable', locale: locale });
  }
  var requestId = 'codex-zh-launcher-' + Date.now() + '-' + Math.random().toString(16).slice(2);
  var response = await new Promise(function (resolve) {
    var timer;
    var onMessage = function (event) {
      var message = event && event.data;
      if (!message || message.type !== 'fetch-response' || message.requestId !== requestId) return;
      window.removeEventListener('message', onMessage);
      window.clearTimeout(timer);
      resolve(message);
    };
    window.addEventListener('message', onMessage);
    timer = window.setTimeout(function () {
      window.removeEventListener('message', onMessage);
      resolve({ responseType: 'timeout' });
    }, 8000);
    Promise.resolve(bridge.sendMessageFromView({
      type: 'fetch',
      requestId: requestId,
      method: 'POST',
      url: 'vscode://codex/set-setting',
      body: JSON.stringify({ key: 'localeOverride', value: locale })
    })).catch(function (error) {
      window.removeEventListener('message', onMessage);
      window.clearTimeout(timer);
      resolve({ responseType: 'error', error: String(error) });
    });
  });
  if (response && response.responseType === 'success') {
    window.setTimeout(function () { window.location.reload(); }, 600);
    return JSON.stringify({ status: 'ok', locale: locale });
  }
  return JSON.stringify({ status: 'partial', locale: locale, response: response });
})()
