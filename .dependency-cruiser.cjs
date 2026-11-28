/**
 * 架构边界约束(Enforceable Architecture)
 *
 * 运行:pnpm lint:arch
 * 规则演进见 docs/governance/TASKS.md (T3/T8/T9)。
 */
/** @type {import('dependency-cruiser').IConfiguration} */
module.exports = {
  forbidden: [
    {
      name: "no-raw-invoke-outside-api-layer",
      severity: "error",
      comment:
        "Tauri invoke 命令契约只允许出现在 src/lib/**(api 层)与 src/tray-panel/**(第二 WebView 入口自带 api 层)。UI 组件/hooks/contexts/入口必须经由 lib/api 的封装,保证命令名与类型只有一处权威定义。",
      from: { pathNot: ["^src/lib/", "^src/tray-panel/"] },
      to: { path: "^(node_modules/)?@tauri-apps/api/core$" },
    },
  ],
  options: {
    /* 了解依赖图时有用: pnpm exec depcruise -T dot src | dot -Tsvg > deps.svg */
    doNotFollow: ["node_modules"],
    tsConfig: { fileName: "tsconfig.json" },
    enhancedResolveOptions: {
      exportsFields: ["exports"],
      conditionNames: ["import", "require", "node", "default"],
    },
    reporterOptions: {
      text: { highlightFocused: true },
    },
  },
};
