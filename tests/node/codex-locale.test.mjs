// Run with node --experimental-vm-modules --test tests/node/codex-locale.test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const resources = new URL("../../src-tauri/resources/codex_zh/", import.meta.url);
const localeScript = readFileSync(new URL("locale-script.js", resources), "utf8")
  .replace("__LOCALE_JSON__", '"zh-CN"');

async function runLocale({ exports = {}, bridge = "success", urls, importFails = false } = {}) {
  let clock = 0;
  let bridgeCalls = 0;
  let reloads = 0;
  const imported = [];
  const timers = new Map();
  const listeners = new Set();
  let timerId = 0;
  const window = {
    location: { hostname: "-", reload() { reloads++; } },
    setTimeout(callback, delay) {
      const id = ++timerId;
      if (delay === 100) callback();
      else timers.set(id, callback);
      return id;
    },
    clearTimeout(id) { timers.delete(id); },
    addEventListener(_, listener) { listeners.add(listener); },
    removeEventListener(_, listener) { listeners.delete(listener); },
    electronBridge: {
      async sendMessageFromView(request) {
        bridgeCalls++;
        assert.equal(request.url, "vscode://codex/set-setting");
        assert.deepEqual(JSON.parse(request.body), { key: "localeOverride", value: "zh-CN" });
        if (bridge === "throw") throw new Error("private diagnostic text");
        for (const listener of [...listeners]) {
          listener({ data: { type: "fetch-response", requestId: request.requestId,
            responseType: bridge, error: "private diagnostic text" } });
        }
      },
    },
  };
  const context = vm.createContext({ window, URL, Set,
    Date: { now() { clock += 1000; return clock; } },
    document: {
      baseURI: "app://-/index.html",
      querySelectorAll() {
        return (urls ?? ["app://-/assets/app-shared-test.js"]).map(href => ({ href }));
      },
    },
  });
  const module = new vm.SyntheticModule(Object.keys(exports), function () {
    for (const [key, value] of Object.entries(exports)) this.setExport(key, value);
  }, { context });
  await module.link(() => {});
  await module.evaluate();
  const script = new vm.Script(localeScript, {
    importModuleDynamically(url) {
      imported.push(url);
      if (importFails) throw new Error("private import error");
      return module;
    },
  });
  const result = JSON.parse(await script.runInContext(context));
  for (const callback of timers.values()) callback();
  assert.equal(listeners.size, 0);
  return { result, bridgeCalls, imported, reloads };
}

function settingsService({ reject = false, effective = "zh-CN" } = {}) {
  const writes = [];
  return {
    writes,
    settings: {
      async write(key, value) {
        writes.push([key, value]);
        if (reject) throw new Error("private settings error");
      },
      async read(key) {
        assert.equal(key, "localeOverride");
        return { effective };
      },
    },
  };
}

for (const name of ["Bct", "RenamedExport", "appServices"]) {
  test(`writes locale with settings export ${name}`, async () => {
    const service = settingsService();
    const actual = await runLocale({ exports: { [name]: service } });
    assert.equal(actual.result.status, "ok");
    assert.equal(actual.bridgeCalls, 0);
    assert.equal(actual.reloads, 1);
    assert.deepEqual(service.writes, [["localeOverride", "zh-CN"]]);
  });
}

test("finds initialized Rspack appServices without requiring other modules", async () => {
  const service = settingsService();
  const runtime = () => { throw new Error("Must not execute module factories"); };
  runtime.c = { renamedModule: { exports: { appServices: service } } };
  const actual = await runLocale({ exports: { __webpack_require__: runtime },
    urls: ["app://-/assets/348410.current.js"] });
  assert.equal(actual.result.status, "ok");
  assert.equal(actual.bridgeCalls, 0);
  assert.equal(service.writes.length, 1);
});

for (const [name, options] of [
  ["missing settings export", {}],
  ["module import failure", { importFails: true }],
  ["settings write failure", { exports: { renamed: settingsService({ reject: true }) } }],
  ["unconfirmed setting", { exports: { renamed: settingsService({ effective: "en-US" }) } }],
]) {
  test(`uses legacy bridge after ${name}`, async () => {
    const actual = await runLocale(options);
    assert.equal(actual.result.status, "ok");
    assert.equal(actual.bridgeCalls, 1);
  });
}

test("imports only current app assets and deduplicates entries", async () => {
  const actual = await runLocale({ urls: [
    "https://example.com/assets/test.js", "file:///assets/test.js",
    "app://codex/assets/other-app.js", "app://-/other/test.js",
    "app://-/assets/test.js?query", "app://-/assets/test.js#fragment",
    "app://-/assets/test.js", "app://-/assets/test.js",
  ] });
  assert.deepEqual(actual.imported, ["app://-/assets/test.js"]);
});

for (const bridge of ["error", "throw"]) {
  test(`reports unconfirmed locale without leaking ${bridge} diagnostics`, async () => {
    const actual = await runLocale({ bridge });
    assert.equal(actual.result.status, "partial");
    assert.equal(actual.reloads, 0);
    assert.ok(!JSON.stringify(actual).includes("private"));
  });
}

test("patches the language gate before the client's first synchronous read", () => {
  const hook = readFileSync(new URL("linux-statsig-hook.js", resources), "utf8");
  const bootstrap = readFileSync(new URL("i18n-bootstrap.js", resources), "utf8");
  const context = vm.createContext({ Navigator: function () {},
    window: { setInterval() { return 1; }, clearInterval() {} } });
  vm.runInContext(hook + bootstrap, context);
  const enabled = vm.runInContext(`
    globalThis.__STATSIG__ = {};
    __STATSIG__.firstInstance = {
      getLayer() { return { get(key, fallback) { return fallback; } }; }
    };
    __STATSIG__.firstInstance.getLayer("72216192").get("enable_i18n", false);
  `, context);
  assert.equal(enabled, true);
});

test("verifies visible shell controls without using words from the open chat", async () => {
  const source = readFileSync(new URL("verify-locale.js", resources), "utf8").replace("__LOCALE_JSON__", '"zh-CN"');
  const control = (text, chat = false, visible = true) => ({
    innerText: text, getAttribute: () => null, closest: () => chat ? {} : null,
    getClientRects: () => visible ? [{}] : [],
  });
  async function check(items, bodyText) {
    let time = 0;
    return JSON.parse(await vm.runInNewContext(source, {
      document: { body: {innerText: bodyText}, querySelectorAll: () => items },
      window: {setTimeout: resolve => resolve()}, Date: {now: () => time += 2000},
    })).status;
  }
  assert.equal(await check([control("插件"), control("设置"), control("Settings", true)], "插件 设置 Settings"), "ok");
  assert.equal(await check([control("Settings"), control("Plugins"), control("插件",true), control("设置",false,false)], "Settings Plugins 插件 设置"), "partial");
  assert.equal(await check([control("插件"), control("插件")], "插件"), "partial");
});

test("confirms the signed-out Chinese Linux desktop from distinct visible login actions", () => {
  const source = readFileSync(new URL("verify-linux-login.js", resources), "utf8");
  const action = (innerText, hidden = false, inChat = false, aria = null) => ({
    innerText, getClientRects: () => hidden ? [] : [{}], closest: () => inChat ? {} : null,
    getAttribute: name => name === "aria-label" ? aria : null,
  });
  const screen = (headings, actions) => vm.runInNewContext(source, {
    document: { querySelectorAll: (selector) => selector.startsWith("h1") ? headings : actions },
  });
  const check = (...actions) => screen([action("登录 ChatGPT")], actions);
  assert.equal(screen([action("聊天")], [action("继续登录"), action("注册")]), false);
  assert.equal(screen([action("登录 ChatGPT", true)], [action("继续登录"), action("注册")]), false);
  assert.equal(check(action("继续登录"), action("使用其他方式登录"), action("注册")), true);
  assert.equal(check(action("", false, false, "继续登录"), action("注册")), true);
  assert.equal(check(action("继续登录", false, false, "注册")), false);
  assert.equal(check(action("继续登录"), action("继续登录")), false);
  assert.equal(check(action("继续登录"), action("注册", true), action("使用其他方式登录", false, true)), false);
  assert.equal(check(action("Continue"), action("Sign up")), false);
});
