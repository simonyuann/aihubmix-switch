import path from "node:path";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    include: ["{src,tests}/**/*.{test,spec}.{ts,tsx,js,jsx}"],
    // 重 jsdom 组件测试(PiProviderForm 等)在全量并发下单测可达 5-8s,
    // 默认 5000ms 会导致负载敏感的伪失败(T17 归因,见 docs/governance)。
    testTimeout: 20000,
    environment: "jsdom",
    setupFiles: ["./tests/setupGlobals.ts", "./tests/setupTests.ts"],
    globals: true,
    coverage: {
      reporter: ["text", "lcov"],
    },
  },
});
