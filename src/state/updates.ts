import { defineStore } from "pinia";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktopInvoke as invoke } from "../services/desktop-api";

export interface UpdateStatus {
  revision: number;
  phase: "idle" | "checking" | "up-to-date" | "available" | "downloading" | "ready" | "installing" | "error";
  currentVersion: string;
  version: string | null;
  notes: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  errorCode: string | null;
  errorMessage: string | null;
  dismissedVersion: string | null;
}

export const useUpdatesStore = defineStore("updates", {
  state: () => ({
    status: {
      revision: -1, phase: "idle", currentVersion: "", version: null, notes: null,
      downloadedBytes: 0, totalBytes: null, errorCode: null, errorMessage: null,
      dismissedVersion: null,
    } as UpdateStatus,
  }),
  getters: {
    busy: (state) => ["checking", "downloading", "installing"].includes(state.status.phase),
  },
  actions: {
    apply(status: UpdateStatus) {
      if (status.revision >= this.status.revision) this.status = status;
    },
    async initialize() {
      if (!isTauri()) return () => {};
      const unlisten = await listen<UpdateStatus>("update-status-changed", ({ payload }) => this.apply(payload));
      try { this.apply(await invoke<UpdateStatus>("get_update_status")); }
      catch (error) { unlisten(); throw error; }
      return unlisten;
    },
    async check() { this.apply(await invoke<UpdateStatus>("check_for_updates")); },
    async download() { this.apply(await invoke<UpdateStatus>("download_update")); },
    async dismiss() {
      const version = this.status.version;
      if (version) await invoke("dismiss_update", { version });
    },
  },
});
