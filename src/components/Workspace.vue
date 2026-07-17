<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { defineShortcuts, useToast } from "@nuxt/ui/composables";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import AppShell from "./AppShell.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import FileLibraryModal from "./FileLibraryModal.vue";
import FileReadyState from "./FileReadyState.vue";
import SettingsModal from "./SettingsModal.vue";
import ViewerHost from "../viewers/ViewerHost.vue";
import { resolveViewer } from "../viewers/registry";
import { createDocumentState, documentState, resetDocument } from "../state/document";
import {
  addRecent,
  isFavorite,
  libraryState,
  loadLibrary,
  removeRecent,
  saveSession,
  toggleFavorite,
} from "../state/library";
import type {
  BinaryDocument,
  DocumentState,
  ImageDocument,
  OpenedFile,
  SaveResult,
  TextDocument,
} from "../types/document";
import { initializeDesktopLifecycle } from "../services/desktop-lifecycle";
import { recordDiagnostic } from "../services/diagnostics";
import { loadSettings } from "../state/settings";

interface WorkspaceTab {
  id: string;
  state: DocumentState;
  fallbackName: string;
}

const toast = useToast();
const tabs = reactive<WorkspaceTab[]>([]);
const activeTabId = ref("");
const dragActive = ref(false);
const guardOpen = ref(false);
const settingsOpen = ref(false);
const libraryOpen = ref(false);
let tabSequence = 0;
let disposeLifecycle: (() => void) | undefined;
let resolveGuard: ((result: boolean) => void) | undefined;

const activeTab = computed(() => tabs.find((tab) => tab.id === activeTabId.value) ?? null);
const dirtyTabs = computed(() => tabs.filter((tab) => tab.state.dirty));
const hasViewer = computed(() => !!documentState.file && !!resolveViewer(documentState.file));
const canSave = computed(() => !!documentState.file && !documentState.readOnly && documentState.status === "ready");
const currentFavorite = computed(() => !!documentState.file && isFavorite(documentState.file.path));

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function syncActiveTab() {
  if (activeTab.value) Object.assign(activeTab.value.state, documentState);
  persistTabs();
}

function persistTabs() {
  saveSession(
    tabs.flatMap((tab) => tab.state.file?.path ? [tab.state.file.path] : []),
    documentState.file?.path ?? null,
  );
}

function activateTab(id: string) {
  if (id === activeTabId.value) return;
  syncActiveTab();
  const target = tabs.find((tab) => tab.id === id);
  if (!target) return;
  activeTabId.value = id;
  Object.assign(documentState, target.state);
  persistTabs();
}

async function loadFileState(state: DocumentState, path: string) {
  const file = await invoke<OpenedFile>("inspect_file", { path });
  const definition = resolveViewer(file);
  if (!definition) throw new Error(`暂不支持 .${file.extension || "未知"} 文件`);

  state.file = file;
  state.viewerKind = file.kind;
  state.sourceModifiedAt = file.modifiedAt;
  state.dirty = false;
  state.truncated = false;

  if (file.kind === "image") {
    const image = await invoke<ImageDocument>("read_image", { path: file.path });
    state.buffer = image.dataUrl;
    state.readOnly = true;
  } else if (["pdf", "docx", "xlsx", "video"].includes(file.kind)) {
    const binary = await invoke<BinaryDocument>("read_binary", { path: file.path });
    state.buffer = binary.base64;
    state.readOnly = true;
  } else {
    const text = await invoke<TextDocument>("read_text", { path: file.path });
    state.buffer = text.content;
    state.encoding = text.encoding;
    state.lineEnding = text.lineEnding;
    state.bom = text.bom;
    state.readOnly = text.readOnly;
    state.truncated = text.truncated;
    file.encoding = text.encoding;
    file.lineEnding = text.lineEnding;
    file.readOnly = text.readOnly;
  }
  state.status = "ready";
  addRecent(file.path, file.name);
}

async function openPathAsTab(path: string) {
  const existing = tabs.find((tab) => tab.state.file?.path === path);
  if (existing) {
    activateTab(existing.id);
    if (existing.state.file) addRecent(existing.state.file.path, existing.state.file.name);
    return;
  }

  const state = createDocumentState();
  state.status = "loading";
  const tab: WorkspaceTab = {
    id: `tab-${Date.now()}-${tabSequence++}`,
    state,
    fallbackName: path.split(/[\\/]/).pop() || "新标签页",
  };
  tabs.push(tab);
  activateTab(tab.id);
  const startedAt = performance.now();
  try {
    await loadFileState(state, path);
    recordDiagnostic("performance", `open-${state.viewerKind}`, performance.now() - startedAt);
  } catch (error) {
    recordDiagnostic("viewer", "open-failed", performance.now() - startedAt);
    state.status = "error";
    state.error = {
      code: "open-failed",
      title: "无法打开文件",
      message: errorMessage(error),
      recoverable: true,
    };
  }
  if (activeTabId.value === tab.id) Object.assign(documentState, state);
  persistTabs();
}

async function openPaths(paths: string[]) {
  for (const path of [...new Set(paths.filter(Boolean))]) await openPathAsTab(path);
}

async function selectFile() {
  if (!isTauri()) {
    documentState.error = {
      code: "desktop-only",
      title: "请在 KView 桌面应用中打开文件",
      message: "浏览器预览不具备本地文件选择权限。",
      recoverable: true,
    };
    return;
  }
  const selection = await open({ multiple: true, directory: false });
  if (selection) await openPaths(Array.isArray(selection) ? selection : [selection]);
}

function updateContent(value: string) {
  if (documentState.readOnly || documentState.buffer === value) return;
  documentState.buffer = value;
  documentState.dirty = true;
  syncActiveTab();
}

async function saveState(state: DocumentState, saveAs: boolean) {
  if (!state.file || state.readOnly || state.buffer === null || state.status !== "ready") return false;
  let targetPath = state.file.path;
  if (saveAs) {
    const selected = await save({
      defaultPath: state.file.path,
      filters: [{ name: "文本文件", extensions: [state.file.extension || "txt"] }],
    });
    if (!selected) return false;
    targetPath = selected;
  }

  try {
    const result = await invoke<SaveResult>("save_text", {
      path: targetPath,
      content: state.buffer,
      encoding: state.encoding ?? "utf-8",
      lineEnding: state.lineEnding ?? "lf",
      bom: state.bom,
      expectedModifiedAt: saveAs ? null : state.sourceModifiedAt,
    });
    const refreshed = await invoke<OpenedFile>("inspect_file", { path: result.path });
    refreshed.encoding = state.encoding;
    refreshed.lineEnding = state.lineEnding;
    refreshed.readOnly = false;
    state.file = refreshed;
    state.sourceModifiedAt = result.modifiedAt;
    state.dirty = false;
    addRecent(refreshed.path, refreshed.name);
    toast.add({ title: saveAs ? "文件已另存" : "文件已保存", description: result.name, color: "success", icon: "i-lucide-check" });
    return true;
  } catch (error) {
    recordDiagnostic("storage", "save-failed");
    toast.add({ title: "保存失败", description: errorMessage(error).replace(/^external-modified:\s*/, ""), color: "error", icon: "i-lucide-triangle-alert", duration: 8000 });
    return false;
  }
}

async function saveDocument(saveAs: boolean) {
  const saved = await saveState(documentState, saveAs);
  syncActiveTab();
  return saved;
}

async function requestDirtyDecision() {
  syncActiveTab();
  if (!dirtyTabs.value.length) return true;
  if (resolveGuard) return false;
  guardOpen.value = true;
  return new Promise<boolean>((resolve) => { resolveGuard = resolve; });
}

function finishGuard(result: boolean) {
  guardOpen.value = false;
  resolveGuard?.(result);
  resolveGuard = undefined;
}

function handleGuardOpenChange(open: boolean) {
  if (!open && resolveGuard) finishGuard(false);
}

async function saveAllFromGuard() {
  for (const tab of [...dirtyTabs.value]) {
    if (!(await saveState(tab.state, false))) return;
  }
  if (activeTab.value) Object.assign(documentState, activeTab.value.state);
  finishGuard(true);
}

async function closeTab(id = activeTabId.value) {
  const index = tabs.findIndex((tab) => tab.id === id);
  if (index < 0) return;
  activateTab(id);
  if (!(await requestDirtyDecision())) return;
  tabs.splice(index, 1);
  activeTabId.value = "";
  if (tabs.length) activateTab(tabs[Math.min(index, tabs.length - 1)].id);
  else resetDocument();
  persistTabs();
}

function cycleTab(direction: number) {
  if (tabs.length < 2) return;
  const index = tabs.findIndex((tab) => tab.id === activeTabId.value);
  activateTab(tabs[(index + direction + tabs.length) % tabs.length].id);
}

function toggleCurrentFavorite() {
  if (!documentState.file) return;
  toggleFavorite(documentState.file.path, documentState.file.name);
}

function clearWorkspace() {
  tabs.splice(0);
  activeTabId.value = "";
  resetDocument();
}

defineShortcuts({
  meta_o: selectFile,
  meta_s: () => saveDocument(false),
  meta_shift_s: () => saveDocument(true),
  meta_w: () => closeTab(),
  meta_tab: () => cycleTab(1),
  meta_shift_tab: () => cycleTab(-1),
});

onMounted(async () => {
  if (!isTauri()) return;
  try {
    await Promise.all([loadSettings(), loadLibrary()]);
  } catch {
    recordDiagnostic("settings", "load-failed");
  }
  disposeLifecycle = await initializeDesktopLifecycle({
    onPaths: openPaths,
    onRequestClose: requestDirtyDecision,
    onBeforeClose: clearWorkspace,
    onDragStateChange: (active) => { dragActive.value = active; },
    onOpenFile: selectFile,
    onCloseTab: () => closeTab(),
    onNextTab: () => cycleTab(1),
    onPreviousTab: () => cycleTab(-1),
  });
  if (!tabs.length && libraryState.sessionPaths.length) {
    await openPaths(libraryState.sessionPaths);
    const restored = tabs.find((tab) => tab.state.file?.path === libraryState.activePath);
    if (restored) activateTab(restored.id);
  }
});

onBeforeUnmount(() => disposeLifecycle?.());
</script>

<template>
  <AppShell :immersive="tabs.length > 0">
    <template #actions>
      <div class="flex items-center gap-1">
        <template v-if="canSave">
          <UBadge v-if="documentState.dirty" color="warning" variant="subtle">未保存</UBadge>
          <UButton color="neutral" variant="ghost" icon="i-lucide-save" label="保存" :disabled="!documentState.dirty" @click="saveDocument(false)" />
        </template>
        <UButton v-if="documentState.file" color="neutral" variant="ghost" :icon="currentFavorite ? 'i-lucide-star-off' : 'i-lucide-star'" :aria-label="currentFavorite ? '取消收藏' : '收藏文件'" @click="toggleCurrentFavorite" />
        <UButton color="neutral" variant="ghost" icon="i-lucide-library" label="文件库" @click="libraryOpen = true" />
        <UButton color="neutral" variant="ghost" icon="i-lucide-folder-open" label="打开" @click="selectFile" />
        <UButton color="neutral" variant="ghost" icon="i-lucide-settings" aria-label="设置" @click="settingsOpen = true" />
      </div>
    </template>

    <template #status>
      <span v-if="documentState.file" class="max-w-96 truncate">{{ documentState.file.path }}</span>
      <span v-else>准备就绪</span>
      <span v-if="tabs.length">{{ tabs.length }} 个标签页 · 文件仅在本机处理</span>
      <span v-else>文件仅在本机处理</span>
    </template>

    <div v-if="tabs.length" class="flex size-full min-h-0 flex-col">
      <div class="flex h-10 shrink-0 items-end gap-1 overflow-x-auto border-b border-muted bg-muted/40 px-2 pt-1">
        <div v-for="tab in tabs" :key="tab.id" class="group flex max-w-56 shrink-0 items-center rounded-t-lg border border-b-0 px-1" :class="tab.id === activeTabId ? 'border-muted bg-default' : 'border-transparent hover:bg-elevated'">
          <button class="flex min-w-0 items-center gap-2 px-2 py-2 text-xs" @click="activateTab(tab.id)">
            <span v-if="tab.state.dirty" class="size-1.5 shrink-0 rounded-full bg-amber-500" />
            <span class="truncate">{{ tab.state.file?.name ?? tab.fallbackName }}</span>
          </button>
          <UButton color="neutral" variant="ghost" size="xs" icon="i-lucide-x" aria-label="关闭标签页" @click.stop="closeTab(tab.id)" />
        </div>
      </div>

      <ErrorState v-if="documentState.error" :error="documentState.error" @dismiss="closeTab()" />
      <div v-else-if="documentState.file && documentState.status === 'ready' && hasViewer" class="flex min-h-0 flex-1 flex-col">
        <UAlert v-if="documentState.truncated" class="m-3 shrink-0" color="warning" variant="subtle" icon="i-lucide-file-warning" title="大文件只读预览" description="文件超过 20 MiB，仅加载前 2 MiB 内容以保持界面流畅。" />
        <KeepAlive :max="12">
          <ViewerHost :key="activeTabId" class="min-h-0 flex-1" :file="documentState.file" :content="documentState.buffer ?? ''" :read-only="documentState.readOnly" @update:content="updateContent" />
        </KeepAlive>
      </div>
      <FileReadyState v-else-if="documentState.file && documentState.status === 'ready'" :file="documentState.file" @open="selectFile" />
      <div v-else-if="documentState.status === 'loading'" class="grid flex-1 place-items-center text-center">
        <div><UIcon name="i-lucide-loader-circle" class="mx-auto size-8 animate-spin text-primary" /><p class="mt-3 text-sm text-muted">正在加载文件…</p></div>
      </div>
    </div>
    <EmptyState v-else :drag-active="dragActive" @select="selectFile" />

    <UModal :open="guardOpen" :dismissible="false" title="保存所有修改？" @update:open="handleGuardOpenChange">
      <template #body><p class="text-sm leading-6 text-muted">共有 {{ dirtyTabs.length }} 个标签页包含未保存修改。保存全部后继续，或放弃与本次关闭操作相关的修改。</p></template>
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <UButton color="neutral" variant="ghost" label="取消" @click="finishGuard(false)" />
          <UButton color="error" variant="soft" label="放弃修改并继续" @click="finishGuard(true)" />
          <UButton icon="i-lucide-save" label="保存全部" @click="saveAllFromGuard" />
        </div>
      </template>
    </UModal>
    <FileLibraryModal v-model:open="libraryOpen" @open="openPathAsTab" @favorite="toggleFavorite" @remove-recent="removeRecent" />
    <SettingsModal v-model:open="settingsOpen" />
  </AppShell>
</template>
