import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { JSDOM } from "jsdom";

const script = readFileSync(
  new URL("../../src-tauri/resources/codex_zh/balance-error.js", import.meta.url),
  "utf8",
);
const endpoint = "https://csmy.fun/v1/responses";
const requestId = "bb9c6ab8-fe78-4a22-83bd-7689317a4382";
const plain = "unexpected status 403 Forbidden: insufficient balance";
const structured = "unexpected status 403 Forbidden: " + JSON.stringify({
  code: "INSUFFICIENT_BALANCE",
  message: "Insufficient account balance",
});

function renderer(t, url = "app://-/index.html") {
  const { window } = new JSDOM(
    '<!doctype html><html lang="zh-CN"><body><main data-turn-transcript></main></body></html>',
    { url, runScripts: "outside-only", pretendToBeVisual: true },
  );
  t.after(() => window.close());
  return {
    doc: window.document,
    install: () => window.eval(script),
    settle: () => new Promise((resolve) =>
      window.requestAnimationFrame(() => window.requestAnimationFrame(resolve)),
    ),
  };
}

// The ordinary Codex error item uses an aside with an outlined warning icon.
// Its message is inline text, interspersed with a separate clickable URL.
function addCard(doc, itemId, message, { link = true } = {}) {
  const row = doc.createElement("div");
  row.setAttribute("data-local-conversation-item-target-ids", itemId);
  row.innerHTML = `<aside><div>
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M8 9.8a.767.767 0 1 1 0 1.533A.767.767 0 0 1 8 9.8Zm0-5.134c.368 0 .667.299.667.667V8a.667.667 0 0 1-1.334 0V5.333c0-.368.299-.667.667-.667Z"/>
      <path d="M8 1.333a6.667 6.667 0 1 1 0 13.334A6.667 6.667 0 0 1 8 1.333Zm0 1.334a5.333 5.333 0 1 0 0 10.666A5.333 5.333 0 0 0 8 2.667Z"/>
    </svg>
    <div><span class="wrap-anywhere"></span></div>
  </div></aside>`;
  const content = row.querySelector("span");
  const anchor = doc.createElement("a");
  anchor.href = endpoint;
  anchor.textContent = endpoint;
  // Adjacent text nodes also occur when inline markdown splits punctuation.
  content.append(message, ", url: ", link ? anchor : endpoint, ", request id: ", requestId);
  doc.querySelector("main").append(row);
  return { row, content, anchor };
}

function assertTranslated(content) {
  assert.match(content.textContent, /账户余额不足/);
  assert.match(content.textContent, /充值/);
  assert.match(content.textContent, /请求\s*ID/i);
  assert.ok(content.textContent.includes(requestId));
  assert.doesNotMatch(content.textContent, /unexpected status|insufficient.balance|request id/i);
}

test("localizes a mounted plain-text 403 and preserves its existing link and request ID", async (t) => {
  const { doc, install, settle } = renderer(t);
  const { content, anchor } = addCard(doc, "e8921c", plain);
  install();
  await settle();
  assertTranslated(content);
  assert.equal(content.querySelector("a"), anchor);
  assert.equal(anchor.href, endpoint);
  assert.equal(anchor.textContent, endpoint);
});

test("localizes the gateway's Chinese reason in plain and structured errors", (t) => {
  const { doc, install } = renderer(t);
  const message = "账户余额不足，请充值后再试。";
  const cards = [message, JSON.stringify({ error: { message } })].map((reason) =>
    addCard(doc, "turn-error-zh", "unexpected status 403 Forbidden: " + reason),
  );
  install();
  for (const { content } of cards) assertTranslated(content);
});

test("localizes newly inserted JSON error cards and a later React text update", async (t) => {
  const { doc, install, settle } = renderer(t, "app://codex/index.html");
  install();
  const { content } = addCard(doc, "turn-error-1", structured, { link: false });
  await settle();
  assertTranslated(content);
  const initiallyUnrelated = "unexpected status 403 Forbidden: permission denied";
  content.textContent = initiallyUnrelated;
  await settle();
  assert.equal(content.textContent, initiallyUnrelated);
  content.firstChild.data = plain + ", url: " + endpoint + ", request id: " + requestId;
  await settle();
  assertTranslated(content);
  assert.ok(content.textContent.includes(endpoint));
});

test("localizes the compact warning layout while preserving later unrelated errors", async (t) => {
  const { doc, install, settle } = renderer(t);
  const header = doc.createElement("div");
  header.className = "group/activity-header";
  header.innerHTML = `<svg viewBox="0 0 16 16"><path d="M8.08398 9.30859C8.49579 9.35064 8.81726 9.69822 8.81738 10.1211C8.81738 10.5724 8.45123 10.9383 8 10.9385C7.57705 10.9384 7.22945 10.617 7.1875 10.2051L7.18262 10.1211L7.1875 10.0381C7.22923 9.62595 7.57689 9.30383 8 9.30371L8.08398 9.30859Z"/></svg>
    <span class="wrap-anywhere whitespace-pre-wrap"></span>`;
  const content = header.querySelector("span");
  const originalSuffix = ", request id: " + requestId;
  content.append(plain, originalSuffix);
  doc.querySelector("main").append(header);
  install();
  await settle();
  assertTranslated(content);
  const other = "unexpected status 403 Forbidden: insufficient permissions";
  // React may reuse the unchanged suffix node when it rerenders the reason.
  content.firstChild.data = other;
  await settle();
  assert.equal(content.textContent, other + originalSuffix);
});

test("recognizes a terminal error after an icon change without translating an ordinary item", async (t) => {
  const { doc, install, settle } = renderer(t);
  const terminal = addCard(doc, "turn-error-example", plain);
  const otherItem = addCard(doc, "ordinary-item", plain);
  for (const item of [terminal, otherItem]) item.row.querySelector("svg").remove();
  const original = otherItem.content.innerHTML;
  install();
  await settle();
  assertTranslated(terminal.content);
  assert.equal(otherItem.content.innerHTML, original);
});

test("leaves permission errors, other HTTP statuses, and quoted conversation text alone", async (t) => {
  const { doc, install, settle } = renderer(t);
  const unrelated = [
    addCard(doc, "turn-error-a", "unexpected status 403 Forbidden: insufficient permissions").content,
    addCard(doc, "turn-error-b", plain.replace("403 Forbidden", "401 Unauthorized")).content,
    addCard(doc, "turn-error-c", 'unexpected status 403 Forbidden: {"code":"PERMISSION_DENIED","message":"insufficient balance"}').content,
  ];
  for (const attr of ["data-conversation-bubble", "data-markdown-text-style"]) {
    const { row } = addCard(doc, "quoted-error", structured);
    const quote = doc.createElement("div");
    quote.setAttribute(attr, "");
    quote.append(row.querySelector("aside"));
    row.append(quote);
    unrelated.push(quote);
  }
  const originals = unrelated.map((el) => el.innerHTML);
  install();
  await settle();
  assert.deepEqual(unrelated.map((el) => el.innerHTML), originals);
});

test("ignores a matching card on a non-Codex origin", async (t) => {
  const { doc, install, settle } = renderer(t, "https://example.test/");
  const { content } = addCard(doc, "turn-error-a", structured);
  const before = content.innerHTML;
  install();
  await settle();
  assert.equal(content.innerHTML, before);
});
