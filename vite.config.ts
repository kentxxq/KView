import { defineConfig, lazyPlugins } from "vite-plus";
import vue from "@vitejs/plugin-vue";
import ui from "@nuxt/ui/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

const ignorePatterns = [
  "src-tauri/**",
  "dist/**",
  "debug/**",
  ".agents/**",
  "docs/**",
  "AGENTS.md",
  "README.md",
  "CHANGLOG.md",
  "plan.md",
  "auto-imports.d.ts",
  "components.d.ts",
];

// https://vite.dev/config/
export default defineConfig({
  plugins: lazyPlugins(() => [
    vue(),
    ui({
      router: false,
      icon: {
        mode: "svg",
        clientBundle: {
          scan: { globInclude: ["src/**/*.{vue,ts,tsx}"] },
        },
      },
      ui: {
        colors: {
          primary: "sky",
          neutral: "slate",
        },
      },
    }),
  ]),

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1800,
    strictPort: true,
    host: host || false,
    headers: {
      "Cache-Control": "no-store",
    },
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1801,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },

  test: {
    include: ["tests/**/*.test.ts"],
    environment: "node",
  },

  fmt: {
    singleQuote: false,
    semi: true,
    trailingComma: "all",
    printWidth: 120,
    ignorePatterns,
  },

  lint: {
    ignorePatterns,
    jsPlugins: [{ name: "vite-plus", specifier: "vite-plus/oxlint-plugin" }],
    rules: {
      "vite-plus/prefer-vite-plus-imports": "error",
    },
    options: {
      typeAware: true,
      typeCheck: true,
    },
  },

  staged: {
    "*": "vp check --fix",
  },
});
