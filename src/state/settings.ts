import { defineStore } from "pinia";
import { isTauri } from "@tauri-apps/api/core";
import { desktopInvoke as invoke } from "../services/desktop-api";

export type ThemePreference = "system" | "light" | "dark";
export type ResidencyMode = "fast" | "low-memory";
export type CloseBehavior = "keep-tabs" | "close-tabs";
export type LogColor = "red" | "orange" | "yellow" | "green" | "blue" | "purple";
export interface LogHighlightRule { keyword: string; color: LogColor }

export interface AppSettings {
  theme: ThemePreference;
  residencyMode: ResidencyMode;
  closeBehavior: CloseBehavior;
  autostart: boolean;
  largeFileThresholdMb: number;
  logHighlightRules: LogHighlightRule[];
}

const defaults = (): AppSettings => ({
  theme: "system",
  residencyMode: "fast",
  closeBehavior: "keep-tabs",
  autostart: true,
  largeFileThresholdMb: 10,
  logHighlightRules: [],
});

let mediaQuery: MediaQueryList | undefined;
let mediaQueryListener: (() => void) | undefined;

export const useSettingsStore = defineStore("settings", {
  state: defaults,
  actions: {
    applyTheme() {
      mediaQuery ??= window.matchMedia("(prefers-color-scheme: dark)");
      const dark = this.theme === "dark" || (this.theme === "system" && mediaQuery.matches);
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.style.colorScheme = dark ? "dark" : "light";
    },
    async load() {
      if (isTauri()) this.$patch(await invoke<AppSettings>("get_settings"));
      this.applyTheme();
      mediaQuery ??= window.matchMedia("(prefers-color-scheme: dark)");
      if (mediaQueryListener) {
        mediaQuery.removeEventListener("change", mediaQueryListener);
      }
      mediaQueryListener = () => this.applyTheme();
      mediaQuery.addEventListener("change", mediaQueryListener);
    },
    async save(settings: AppSettings) {
      const saved = isTauri()
        ? await invoke<AppSettings>("update_settings", { settings })
        : settings;
      this.$patch(saved);
      this.applyTheme();
    },
  },
});
