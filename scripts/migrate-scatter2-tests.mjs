// T22 收尾子批:测试断言迁移(空资源下 t 返回 key 且插值不生效)
import fs from "node:fs";
function patch(p, pairs) {
  let s = fs.readFileSync(p, "utf8");
  let ok = 0;
  const missed = [];
  for (const [o, n] of pairs) {
    if (s.includes(o)) { s = s.split(o).join(n); ok++; }
    else missed.push(o.slice(0, 64).replace(/\n/g, "\\n"));
  }
  fs.writeFileSync(p, s);
  console.log(`${p}: ${ok}/${pairs.length}${missed.length ? " MISSED:\n  " + missed.join("\n  ") : ""}`);
}

patch("tests/components/aihubmix/announcements.test.tsx", [
  [`/新公告.*维护通知/`, `/announcements\\.banner\\.new.*维护通知/`],
  [`/维护通知.*未读/`, `/维护通知.*announcements\\.aria\\.unread/`],
  [`/新公告/`, `/announcements\\.banner\\.new/`],
  [`name: "返回公告列表"`, `name: "announcements.action.backToList"`],
]);

patch("tests/integration/App.test.tsx", [
  [`/新公告.*维护通知/`, `/announcements\\.banner\\.new.*维护通知/`],
  [`await screen.findByRole("heading", { name: "公告" })`, `await screen.findByRole("heading", { name: "announcements.title" })`],
]);

patch("tests/components/aihubmix/api-management.test.tsx", [
  [`name: "删除 开发工具"`, `name: "accountPage.aria.deleteKey"`],
  [`toHaveTextContent(
      "已删除 API Key「开发工具」",
    )`, `toHaveTextContent("accountPage.notice.keyDeleted")`],
  [`toHaveTextContent(
      "请刷新列表确认删除结果",
    )`, `toHaveTextContent("accountPage.error.deleteUnconfirmedDetail")`],
]);

patch("tests/components/aihubmix/codex-desktop-controls.test.tsx", [
  [`name: "启动 Codex"`, `name: "codexDesktop.action.launch"`],
  [`/未能自动退出 Codex，请保存工作并手动退出后重试/`, `/codexDesktop\\.launch\\.manualQuitRetry/`],
]);
console.log("test assertion migration done");
