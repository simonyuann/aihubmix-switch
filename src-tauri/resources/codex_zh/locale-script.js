(async function () {
  var locale = __LOCALE_JSON__;
  // Discover the live settings service by its API. Bundled export names change
  // between releases, and newer Codex builds use an Rspack module cache.
  function settingsFromExports(exports) {
    if (!exports || typeof exports !== 'object') return null;
    for (var key of Object.keys(exports)) {
      try {
        var service = exports[key];
        var settings = service && service.settings;
        if (settings && typeof settings.write === 'function' && typeof settings.read === 'function') {
          return settings;
        }
      } catch (_) {}
    }
    return null;
  }
  function findSettings(modules) {
    for (var module of modules) {
      var settings = settingsFromExports(module);
      if (settings) return settings;
      var runtime = module.__webpack_require__;
      var cache = typeof runtime === 'function' && runtime.c;
      if (!cache || typeof cache !== 'object') continue;
      // Inspect only initialized modules. Requiring unrelated modules can run
      // application code and is unnecessary for changing the locale.
      for (var key of Object.keys(cache)) {
        settings = settingsFromExports(cache[key] && cache[key].exports);
        if (settings) return settings;
      }
    }
    return null;
  }
  var modules = [];
  var loaded = new Set();
  var links = document.querySelectorAll('link[rel="modulepreload"][href*="app-shared-"], script[type="module"][src]');
  for (var link of links) {
    try {
      var url = new URL(link.href || link.src, document.baseURI);
      if (url.protocol !== 'app:' || !['-', 'codex'].includes(url.hostname) ||
          url.hostname !== window.location.hostname || !url.pathname.startsWith('/assets/') ||
          url.search || url.hash || loaded.has(url.href)) continue;
      loaded.add(url.href);
      modules.push(await import(url.href));
    } catch (_) {}
  }
  if (modules.length) {
    var deadline = Date.now() + 8000;
    var settings = findSettings(modules);
    while (!settings && Date.now() < deadline) {
      await new Promise(function (resolve) { window.setTimeout(resolve, 100); });
      settings = findSettings(modules);
    }
    if (settings) {
      try {
        await settings.write('localeOverride', locale);
        var current = await settings.read('localeOverride');
        if (current && current.effective === locale) {
          window.setTimeout(function () { window.location.reload(); }, 600);
          return JSON.stringify({ status: 'ok', locale: locale, method: 'settings-service' });
        }
      } catch (_) {}
    }
  }
  // Older clients expose the setting through the renderer bridge. Try it when
  // a module is absent, renamed, still initializing, or rejects the setting.
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
      resolve({ responseType: 'error' });
    });
  });
  if (response && response.responseType === 'success') {
    window.setTimeout(function () { window.location.reload(); }, 600);
    return JSON.stringify({ status: 'ok', locale: locale });
  }
  return JSON.stringify({ status: 'partial', locale: locale, reason: 'locale-setting-unconfirmed' });
})()
