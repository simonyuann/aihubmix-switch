(function () {
  // Register the client patch before Statsig's first synchronous getLayer.
  globalThis.__codexZhInstallStatsigHook = function (patchClient, patchGlobal) {
    function watchGlobal(statsig) {
      if (!statsig || typeof statsig !== 'object') return;
      patchGlobal(statsig);
      try {
        var descriptor = Object.getOwnPropertyDescriptor(statsig, 'firstInstance');
        if (descriptor && (!descriptor.configurable || descriptor.get || descriptor.set)) return;
        var first = statsig.firstInstance;
        Object.defineProperty(statsig, 'firstInstance', {
          configurable: true,
          enumerable: descriptor ? descriptor.enumerable : true,
          get: function () { return first; },
          set: function (client) { first = client; patchClient(client); }
        });
      } catch (_) {}
    }
    try {
      var descriptor = Object.getOwnPropertyDescriptor(globalThis, '__STATSIG__');
      if (!descriptor || (descriptor.configurable && !descriptor.get && !descriptor.set)) {
        var current = globalThis.__STATSIG__;
        watchGlobal(current);
        Object.defineProperty(globalThis, '__STATSIG__', {
          configurable: true,
          enumerable: descriptor ? descriptor.enumerable : true,
          get: function () { return current; },
          set: function (value) { current = value; watchGlobal(value); }
        });
      }
    } catch (_) {}
  };
})();
