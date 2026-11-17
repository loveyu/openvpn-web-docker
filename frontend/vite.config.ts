import { fileURLToPath } from "node:url";
import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const env = loadEnv(
    mode,
    fileURLToPath(new URL("../", import.meta.url)),
    "WEB_PORT",
  );
  const target = `http://127.0.0.1:${env.WEB_PORT || "18080"}`;

  return {
    plugins: [react()],
    server: {
      proxy: {
        // 保留浏览器请求的 Host，让写操作通过后端的同源校验。
        "/api": { target, changeOrigin: false, ws: true },
        "/healthz": { target, changeOrigin: false },
      },
    },
  };
});
