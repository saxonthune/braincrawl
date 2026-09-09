import { defineConfig, lazyPlugins } from "vite-plus";
import solid from "vite-plugin-solid";

export default defineConfig({
  base: "./",
  fmt: {},
  lint: {
    jsPlugins: [{ name: "vite-plus", specifier: "vite-plus/oxlint-plugin" }],
    rules: { "vite-plus/prefer-vite-plus-imports": "error" },
    options: { typeAware: true, typeCheck: true },
  },
  plugins: lazyPlugins(() => [solid()]),
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8787",
      "/works": "http://127.0.0.1:8787",
      "/graph": "http://127.0.0.1:8787",
      "/stats": "http://127.0.0.1:8787",
      "/edges": "http://127.0.0.1:8787",
      "/health": "http://127.0.0.1:8787",
    },
  },
});
