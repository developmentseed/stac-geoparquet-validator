/// <reference types="vitest/config" />
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "VITE_");
  const target = env.VITE_API_PROXY;
  return {
    plugins: [react()],
    server: {
      proxy: target ? { "/api": { target, changeOrigin: true } } : undefined,
    },
    test: {
      environment: "jsdom",
      alias: {
        "@devseed-ui/collecticons-chakra":
          "@devseed-ui/collecticons-chakra/dist/index.esm.js",
      },
      server: {
        deps: {
          inline: ["@devseed-ui/collecticons-chakra"],
        },
      },
    },
  };
});
