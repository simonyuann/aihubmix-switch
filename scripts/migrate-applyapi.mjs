// T22 ApplyApiDialog 批次:硬编码中文 → applyApiDialog.* keys
import fs from "node:fs";
const p = "src/components/aihubmix/ApplyApiDialog.tsx";
let s = fs.readFileSync(p, "utf8");

const pairs = [
  [`          setError(friendlyError(e, "加载分组失败，请重试。"));`, `          setError(friendlyError(e, t("applyApiDialog.error.loadGroups")));`],
  [`        const warning = friendlyError(
          result.warning,
          "本机凭据保存失败，请返回 API 列表查看。",
        );
        setError(\`API 已申请成功。\${warning}\`);`, `        const warning = friendlyError(
          result.warning,
          t("applyApiDialog.error.credentialSaveFailed"),
        );
        setError(\`\${t("applyApiDialog.success.applied")}\${warning}\`);`],
  [`          \`\${friendlyError(e, "申请结果未能确认")}。请先刷新 Key 列表确认结果，再决定是否重新申请。\``, `          \`\${friendlyError(e, t("applyApiDialog.error.unconfirmed"))}。\${t("applyApiDialog.error.refreshKeysFirst")}\``],
  [`          <DialogTitle>申请 API</DialogTitle>
          <DialogDescription>
            为当前账号申请平台 Key，随后在对应分组卡片中选择此 Key。
          </DialogDescription>`, `          <DialogTitle>{t("applyApiDialog.title")}</DialogTitle>
          <DialogDescription>
            {t("applyApiDialog.description")}
          </DialogDescription>`],
  [`            <span>名称</span>`, `            <span>{t("applyApiDialog.label.name")}</span>`],
  [`              aria-label="API 名称"
              placeholder="例如：我的开发工具"`, `              aria-label={t("applyApiDialog.label.apiName")}
              placeholder={t("applyApiDialog.placeholder.nameExample")}`],
  [`            <span>模型服务</span>
            <select
              aria-label="模型服务"`, `            <span>{t("applyApiDialog.label.modelService")}</span>
            <select
              aria-label={t("applyApiDialog.label.modelService")}`],
  [`              当前暂无可申请的模型服务，请联系平台客服。`, `              {t("applyApiDialog.empty.noServices")}`],
  [`              {uncertain ? "返回查看 Key" : "取消"}`, `              {uncertain
                ? t("applyApiDialog.action.backToKeys")
                : t("applyApiDialog.action.cancel")}`],
  [`              {busy ? "正在申请…" : "确认申请"}`, `              {busy
                ? t("applyApiDialog.status.applying")
                : t("applyApiDialog.action.confirm")}`],
  [`import { friendlyError } from "./friendlyError";`, `import { friendlyError } from "./friendlyError";
import { useTranslation } from "react-i18next";`],
  [`  const [groups, setGroups] = useState<GroupInfo[]>([]);`, `  const { t } = useTranslation();
  const [groups, setGroups] = useState<GroupInfo[]>([]);`],
];

let applied = 0;
const missed = [];
for (const [oldStr, newStr] of pairs) {
  if (s.includes(oldStr)) {
    s = s.split(oldStr).join(newStr);
    applied++;
  } else missed.push(oldStr.slice(0, 60).replace(/\n/g, "\\n"));
}
fs.writeFileSync(p, s);
console.log(`applied: ${applied}/${pairs.length}`);
if (missed.length) {
  console.log("MISSED:");
  for (const m of missed) console.log("  -", m);
}
