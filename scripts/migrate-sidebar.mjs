// T22 Sidebar 子批:7 个导航 label → nav.*(nav.settings 复用既有 key)
// 注:其余 ~31 行 CJK 为中文注释(开发者向),非 i18n 范围,保留。
import fs from "node:fs";

// 1. 组件迁移
{
  const p = "src/components/shell/Sidebar.tsx";
  let s = fs.readFileSync(p, "utf8");
  const pairs = [
    [`        label="首页"`, `        label={t("nav.home")}`],
    [`        label="公告"`, `        label={t("nav.announcements")}`],
    [`        label="帮助与答疑"`, `        label={t("nav.help")}`],
    [`        label="设置"`, `        label={t("nav.settings")}`],
    [`            label="通用设置"`, `            label={t("nav.generalSettings")}`],
    [`            label="关于与更新"`, `            label={t("nav.aboutUpdates")}`],
    [`    { page: "announcements", label: "公告", icon: Megaphone },`, `    { page: "announcements", label: t("nav.announcements"), icon: Megaphone },`],
    // SimpleDirectory 补 hook
    [`}: DirectoryProps) {
  const { hasUpdate } = useUpdate();`, `}: DirectoryProps) {
  const { t } = useTranslation();
  const { hasUpdate } = useUpdate();`],
  ];
  let ok = 0;
  const missed = [];
  for (const [o, n] of pairs) {
    if (s.includes(o)) { s = s.split(o).join(n); ok++; }
    else missed.push(o.slice(0, 50).replace(/\n/g, "\\n"));
  }
  fs.writeFileSync(p, s);
  console.log(`Sidebar.tsx: ${ok}/${pairs.length}${missed.length ? " MISSED:\n  " + missed.join("\n  ") : ""}`);
}

// 2. locale keys(5 个新 key × 5 locale,nav.* 深合并)
const additions = {
  home: { zh: "首页", en: "Home", ja: "ホーム", tr: "Ana sayfa", tw: "首頁" },
  announcements: { zh: "公告", en: "Announcements", ja: "お知らせ", tr: "Duyurular", tw: "公告" },
  help: { zh: "帮助与答疑", en: "Help & FAQ", ja: "ヘルプとFAQ", tr: "Yardım ve SSS", tw: "幫助與答疑" },
  generalSettings: { zh: "通用设置", en: "General", ja: "一般設定", tr: "Genel", tw: "一般設定" },
  aboutUpdates: { zh: "关于与更新", en: "About & updates", ja: "アプリについて", tr: "Hakkında", tw: "關於與更新" },
};
const files = {
  zh: "src/i18n/locales/zh.json",
  en: "src/i18n/locales/en.json",
  ja: "src/i18n/locales/ja.json",
  tr: "src/i18n/locales/tr.json",
  "zh-TW": "src/i18n/locales/zh-TW.json",
};
for (const [locale, file] of Object.entries(files)) {
  const data = JSON.parse(fs.readFileSync(file, "utf8"));
  const field = locale === "zh-TW" ? "tw" : locale;
  for (const [k, v] of Object.entries(additions)) data.nav[k] = v[field];
  fs.writeFileSync(file, JSON.stringify(data, null, 2) + "\n");
}
console.log("locale keys added (nav +5)");

// 3. 测试断言(仅 App.test 真 Sidebar;announcements.test 的首页/公告为测试自建 stub,不动)
{
  const p = "tests/integration/App.test.tsx";
  let s = fs.readFileSync(p, "utf8");
  s = s.split(`name: "设置"`).join(`name: "nav.settings"`);
  s = s.split(`name: "公告",`).join(`name: "nav.announcements",`);
  fs.writeFileSync(p, s);
  console.log("App.test assertions migrated");
}
