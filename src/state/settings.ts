import { reactive } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";

export type ThemePreference = "system" | "light" | "dark";
export type ResidencyMode = "fast" | "low-memory";

export interface AppSettings {
  theme: ThemePreference;
  residencyMode: ResidencyMode;
  autostart: boolean;
}

export const appSettings = reactive<AppSettings>({
  theme: "system",
  residencyMode: "fast",
  autostart: true,
});

let mediaQuery: MediaQueryList | undefined;

export function applyTheme() {
  mediaQuery ??= window.matchMedia("(prefers-color-scheme: dark)");
  const dark = appSettings.theme === "dark" || (appSettings.theme === "system" && mediaQuery.matches);
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

export async function loadSettings() {
  if (isTauri()) Object.assign(appSettings, await invoke<AppSettings>("get_settings"));
  applyTheme();
  mediaQuery ??= window.matchMedia("(prefers-color-scheme: dark)");
  mediaQuery.addEventListener("change", applyTheme);
}

export async function saveSettings(settings: AppSettings) {
  const saved = isTauri()
    ? await invoke<AppSettings>("update_settings", { settings })
    : settings;
  Object.assign(appSettings, saved);
  applyTheme();
}
