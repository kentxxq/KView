import { isTauri } from "@tauri-apps/api/core";
import { desktopInvoke as invoke } from "./desktop-api";

export function recordDiagnostic(category: string, code: string, durationMs?: number) {
  if (!isTauri()) return;
  void invoke("record_diagnostic", { category, code, durationMs }).catch(() => undefined);
}

export function installGlobalDiagnostics() {
  window.addEventListener("error", () => recordDiagnostic("runtime", "unhandled-error"));
  window.addEventListener("unhandledrejection", () =>
    recordDiagnostic("runtime", "unhandled-rejection"),
  );
}
