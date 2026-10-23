import { defineStore } from "pinia";
import type { DocumentState } from "../types/document";

export interface WorkspaceTab {
  id: string;
  sourceKey: string;
  state: DocumentState;
  fallbackName: string;
  comparison?: { leftId: string; rightId: string; focused: "left" | "right" };
  logMode?: boolean;
}

export const useWorkspaceStore = defineStore("workspace", {
  state: () => ({
    tabs: [] as WorkspaceTab[],
    activeTabId: "",
  }),
  getters: {
    activeTab: (state) => state.tabs.find((tab) => tab.id === state.activeTabId) ?? null,
  },
  actions: {
    activate(id: string) {
      if (this.tabs.some((tab) => tab.id === id)) this.activeTabId = id;
    },
    add(tab: WorkspaceTab) {
      this.tabs.push(tab);
      this.activeTabId = tab.id;
    },
    remove(ids: string[]) {
      const targets = new Set(ids);
      for (let index = this.tabs.length - 1; index >= 0; index--) {
        if (targets.has(this.tabs[index].id)) this.tabs.splice(index, 1);
      }
      if (!this.tabs.some((tab) => tab.id === this.activeTabId)) {
        this.activeTabId = this.tabs[this.tabs.length - 1]?.id ?? "";
      }
    },
    clear() {
      this.tabs.splice(0);
      this.activeTabId = "";
    },
  },
});
