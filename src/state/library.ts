import { defineStore } from "pinia";
import { isTauri } from "@tauri-apps/api/core";
import { desktopInvoke as invoke } from "../services/desktop-api";

export interface LibraryEntry {
  path: string;
  name: string;
  timestamp: number;
}

interface StoredLibraryV1 {
  recent?: unknown;
  favorites?: unknown;
  sessionPaths?: unknown;
  activePath?: unknown;
}

interface StoredLibraryV2 {
  schemaVersion: 2;
  recent: LibraryEntry[];
  favorites: LibraryEntry[];
  sessionPaths: string[];
  activePath: string | null;
}

const STORAGE_KEY = "kview-library-v2";
const LEGACY_STORAGE_KEY = "kview-library-v1";

function validEntry(value: unknown): value is LibraryEntry {
  if (!value || typeof value !== "object") return false;
  const entry = value as Partial<LibraryEntry>;
  return typeof entry.path === "string"
    && typeof entry.name === "string"
    && typeof entry.timestamp === "number";
}

function parseStored(value: string | null): StoredLibraryV2 | null {
  if (!value) return null;
  try {
    const stored = JSON.parse(value) as StoredLibraryV1 & { schemaVersion?: unknown };
    return {
      schemaVersion: 2,
      recent: Array.isArray(stored.recent) ? stored.recent.filter(validEntry).slice(0, 30) : [],
      favorites: Array.isArray(stored.favorites) ? stored.favorites.filter(validEntry) : [],
      sessionPaths: Array.isArray(stored.sessionPaths)
        ? stored.sessionPaths.filter((path): path is string => typeof path === "string")
        : [],
      activePath: typeof stored.activePath === "string" ? stored.activePath : null,
    };
  } catch {
    return null;
  }
}

export const useLibraryStore = defineStore("library", {
  state: (): StoredLibraryV2 => ({
    schemaVersion: 2,
    recent: [],
    favorites: [],
    sessionPaths: [],
    activePath: null,
  }),
  actions: {
    persist() {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.$state));
    },
    async load() {
      const current = parseStored(localStorage.getItem(STORAGE_KEY));
      const legacy = current ? null : parseStored(localStorage.getItem(LEGACY_STORAGE_KEY));
      const stored = current ?? legacy;
      if (stored) this.$patch(stored);
      if (isTauri()) {
        try {
          const paths = [...new Set([
            ...this.recent.map((entry) => entry.path),
            ...this.favorites.map((entry) => entry.path),
            ...this.sessionPaths,
          ])];
          const existing = new Set(await invoke<string[]>("existing_paths", { paths }));
          this.recent = this.recent.filter((entry) => existing.has(entry.path));
          this.favorites = this.favorites.filter((entry) => existing.has(entry.path));
          this.sessionPaths = this.sessionPaths.filter((path) => existing.has(path));
          if (this.activePath && !existing.has(this.activePath)) this.activePath = null;
        } catch {
          return;
        }
      }
      this.persist();
      if (legacy) localStorage.removeItem(LEGACY_STORAGE_KEY);
    },
    addRecent(path: string, name: string) {
      this.recent = [
        { path, name, timestamp: Date.now() },
        ...this.recent.filter((entry) => entry.path !== path),
      ].slice(0, 30);
      this.persist();
    },
    toggleFavorite(path: string, name: string) {
      const exists = this.favorites.some((entry) => entry.path === path);
      this.favorites = exists
        ? this.favorites.filter((entry) => entry.path !== path)
        : [{ path, name, timestamp: Date.now() }, ...this.favorites];
      this.persist();
    },
    isFavorite(path: string) {
      return this.favorites.some((entry) => entry.path === path);
    },
    removeRecent(path: string) {
      this.recent = this.recent.filter((entry) => entry.path !== path);
      this.persist();
    },
    saveSession(paths: string[], activePath: string | null) {
      this.sessionPaths = [...new Set(paths)];
      this.activePath = activePath;
      this.persist();
    },
  },
});
