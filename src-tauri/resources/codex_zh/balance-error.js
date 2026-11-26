// Presentation-only translation for the error rows in the Chinese Codex window.
(() => {
  if (window.top !== window || location.protocol !== 'app:' ||
      !['-', 'codex'].includes(location.host) || window.__aihubBalanceErrorObserver) return;

  const label = 'span.wrap-anywhere';
  const excluded = '[data-markdown-text-style], [data-conversation-bubble], pre, code, [contenteditable="true"]';
  const rowSelector = 'aside, [class~="group/activity-header"]';
  const pattern = /^unexpected status 403(?: forbidden)?:\s*(.+?)(?:,\s*url:\s*(https?:\/\/\S+?))?(?:,\s*request id:\s*([\w.:-]+))?$/is;
  const balanceMessage = /^(?:insufficient (?:account )?balance\.?|账户余额不足，请充值后再试。)$/i;
  const translated = new WeakMap();

  function isBalanceReason(reason) {
    if (balanceMessage.test(reason.trim())) return true;
    if (!reason.startsWith('{')) return false;
    try {
      const body = JSON.parse(reason);
      const error = body.error ?? body;
      return error.code != null
        ? String(error.code).toUpperCase() === 'INSUFFICIENT_BALANCE'
        : typeof error.message === 'string' && balanceMessage.test(error.message.trim());
    } catch {
      return false;
    }
  }

  // React can reuse individual text nodes on reconnect. Keep their source text
  // so a later, unrelated error can render normally, even after a partial update.
  function source(node) {
    const prior = translated.get(node);
    return prior && prior.display === node.data ? prior.original : node.textContent;
  }

  function write(node, display) {
    const original = source(node);
    if (display === original) translated.delete(node);
    else translated.set(node, { original, display });
    if (node.data !== display) node.data = display;
  }

  function writeTextRun(nodes, text) {
    nodes.forEach((node, index) => write(node, index === 0 ? text : ''));
  }

  function localize(span) {
    const row = span.closest(rowSelector);
    if (!row || span.closest(excluded)) return;
    // Terminal turn errors can carry a typed item ID. Streamed error cards
    // have arbitrary IDs and use Codex's warning glyph in either row layout.
    const itemIds = row.closest('[data-local-conversation-item-target-ids]')
      ?.getAttribute('data-local-conversation-item-target-ids') || '';
    const icon = row.matches('aside') ? 'M8 9.8a.767' : 'M8.08398 9.30859';
    if (!/^turn-error-\S+$/.test(itemIds) && !row.querySelector(`svg path[d^="${icon}"]`)) return;
    const nodes = Array.from(span.childNodes);
    const match = pattern.exec(nodes.map(source).join('').trim());
    if (!match || !isBalanceReason(match[1])) {
      for (const node of nodes) if (translated.has(node)) write(node, source(node));
      return;
    }
    const [, , url, requestId] = match;
    const message = '账户余额不足（HTTP 403），请充值后再试。';
    const idText = requestId ? '请求 ID：' + requestId : '';
    const linkIndex = nodes.findIndex(node => node.nodeType !== Node.TEXT_NODE);
    if (linkIndex < 0) {
      const details = [url && '请求地址：' + url, idText].filter(Boolean).join('，');
      writeTextRun(nodes, message + details);
      return;
    }
    // Codex renders an inline URL as a separate, interactive element.
    // Keep that element, its handlers, and React's surrounding nodes intact.
    const tail = nodes.slice(linkIndex + 1);
    if (!url || linkIndex === 0 || nodes[linkIndex].textContent !== url ||
        tail.some(node => node.nodeType !== Node.TEXT_NODE)) return;
    writeTextRun(nodes.slice(0, linkIndex), message + '请求地址：');
    writeTextRun(tail, idText && '，' + idText);
  }

  // Process only inserted subtrees and changed error labels during streaming.
  const observer = new MutationObserver(records => {
    const labels = new Set();
    for (const record of records) {
      const el = record.target.nodeType === Node.ELEMENT_NODE ? record.target : record.target.parentElement;
      const owner = el?.closest(label);
      if (owner) labels.add(owner);
      for (const node of record.addedNodes) {
        if (node.nodeType !== Node.ELEMENT_NODE || owner || node.closest(excluded)) continue;
        if (node.matches(label)) labels.add(node);
        for (const child of node.querySelectorAll(label)) labels.add(child);
      }
    }
    for (const span of labels) localize(span);
  });
  window.__aihubBalanceErrorObserver = observer;
  observer.observe(document, { childList: true, characterData: true, subtree: true });
  document.querySelectorAll(label).forEach(localize);
})();
