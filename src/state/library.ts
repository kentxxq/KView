import { reactive } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";

export interface LibraryEntry {
  path: string;
  name: string;
  timestamp: number;
}

interface StoredLibrary {
  recent: LibraryEntry[];
  favorites: LibraryEntry[];
  sessionPaths: string[];
  activePath: string | null;
}

const STORAGE_KEY = "kview-library-v1";

export const libraryState = reactive<StoredLibrary>({
  recent: [],
  favorites: [],
  sessionPaths: [],
  activePath: null,
});

function persist() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(libraryState));
}

export async function loadLibrary() {
  try {
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<StoredLibrary> | null;
    if (stored) {
      libraryState.recent = Array.isArray(stored.recent) ? stored.recent.slice(0, 30) : [];
      libraryState.favorites = Array.isArray(stored.favorites) ? stored.favorites : [];
      libraryState.sessionPaths = Array.isArray(stored.sessionPaths) ? stored.sessionPaths : [];
      libraryState.activePath = typeof stored.activePath === "string" ? stored.activePath : null;
    }
    if (isTauri()) {
      const paths = [...new Set([
        ...libraryState.recent.map((entry) => entry.path),
        ...libraryState.favorites.map((entry) => entry.path),
        ...libraryState.sessionPaths,
      ])];
      const existing = new Set(await invoke<string[]>("existing_paths", { paths }));
      libraryState.recent = libraryState.recent.filter((entry) => existing.has(entry.path));
      libraryState.favorites = libraryState.favorites.filter((entry) => existing.has(entry.path));
      libraryState.sessionPaths = libraryState.sessionPaths.filter((path) => existing.has(path));
      if (libraryState.activePath && !existing.has(libraryState.activePath)) libraryState.activePath = null;
    }
  } catch {
    libraryState.recent = [];
    libraryState.favorites = [];
    libraryState.sessionPaths = [];
    libraryState.activePath = null;
  }
  persist();
}

export function addRecent(path: string, name: string) {
  libraryState.recent = [
    { path, name, timestamp: Date.now() },
    ...libraryState.recent.filter((entry) => entry.path !== path),
  ].slice(0, 30);
  persist();
}

export function toggleFavorite(path: string, name: string) {
  const exists = libraryState.favorites.some((entry) => entry.path === path);
  libraryState.favorites = exists
    ? libraryState.favorites.filter((entry) => entry.path !== path)
    : [{ path, name, timestamp: Date.now() }, ...libraryState.favorites];
  persist();
}

export function isFavorite(path: string) {
  return libraryState.favorites.some((entry) => entry.path === path);
}

export function removeRecent(path: string) {
  libraryState.recent = libraryState.recent.filter((entry) => entry.path !== path);
  persist();
}

export function saveSession(paths: string[], activePath: string | null) {
  libraryState.sessionPaths = [...new Set(paths)];
  libraryState.activePath = activePath;
  persist();
}

