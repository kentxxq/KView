<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { defineShortcuts, useToast } from "@nuxt/ui/composables";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import AppShell from "./AppShell.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import FileLibraryModal from "./FileLibraryModal.vue";
import FileReadyState from "./FileReadyState.vue";
import SettingsModal from "./SettingsModal.vue";
import ViewerFooter from "./ViewerFooter.vue";
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
  ArchiveDocument,
  DocumentState,
  HtmlDocument,
  ImageDocument,
  OpenedFile,
  SaveResult,
  TextReadResult,
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
const tabStrip = ref<HTMLElement>();
const canScrollTabsLeft = ref(false);
const canScrollTabsRight = ref(false);
let tabSequence = 0;
let disposeLifecycle: (() => void) | undefined;
let resolveGuard: ((result: boolean) => void) | undefined;
let tabResizeObserver: ResizeObserver | undefined;

const activeTab = computed(() => tabs.find((tab) => tab.id === activeTabId.value) ?? null);
const dirtyTabs = computed(() => tabs.filter((tab) => tab.state.dirty));
const hasViewer = computed(() => !!documentState.file && !!resolveViewer(documentState.file));
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
  void nextTick(scrollActiveTabIntoView);
}

function updateTabScrollState() {
  const element = tabStrip.value;
  if (!element) {
    canScrollTabsLeft.value = false;
    canScrollTabsRight.value = false;
    return;
  }
  canScrollTabsLeft.value = element.scrollLeft > 1;
  canScrollTabsRight.value = element.scrollLeft + element.clientWidth < element.scrollWidth - 1;
}

function scrollActiveTabIntoView() {
  const element = tabStrip.value;
  const activeElement = element?.querySelector<HTMLElement>(`[data-tab-id="${activeTabId.value}"]`);
  activeElement?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
  window.setTimeout(updateTabScrollState, 180);
}

function handleTabWheel(event: WheelEvent) {
  const element = tabStrip.value;
  if (!element || element.scrollWidth <= element.clientWidth) return;
  event.preventDefault();
  element.scrollLeft += Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
  updateTabScrollState();
}

function tabContextItems(tab: WorkspaceTab) {
  const path = tab.state.file?.path;
  return [
    {
      label: "打开文件位置",
      icon: "i-lucide-folder-open",
      disabled: !path,
      onSelect: () => path && revealFile(path),
    },
    {
      label: "复制路径",
      icon: "i-lucide-copy",
      disabled: !path,
      onSelect: () => path && copyFilePath(path),
    },
    { type: "separator" as const },
    {
      label: "打开方式",
      icon: "i-lucide-external-link",
      disabled: !path,
      onSelect: () => path && openFileWith(path),
    },
  ];
}

async function revealFile(path: string) {
  try {
    await invoke("reveal_file", { path });
  } catch (error) {
    toast.add({ title: "无法打开文件位置", description: errorMessage(error), color: "error", icon: "i-lucide-triangle-alert" });
  }
}

async function copyFilePath(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    toast.add({ title: "路径已复制", description: path, color: "success", icon: "i-lucide-copy" });
  } catch (error) {
    toast.add({ title: "无法复制路径", description: errorMessage(error), color: "error", icon: "i-lucide-triangle-alert" });
  }
}

async function openFileWith(path: string) {
  try {
    await invoke("open_file_with", { path });
  } catch (error) {
    toast.add({ title: "无法选择打开方式", description: errorMessage(error), color: "error", icon: "i-lucide-triangle-alert" });
  }
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
  } else if (file.kind === "docx" && file.extension === "doc") {
    const document = await invoke<HtmlDocument>("read_legacy_doc", { path: file.path });
    state.buffer = document.html;
    state.readOnly = true;
  } else if (file.kind === "archive") {
    const archive = await invoke<ArchiveDocument>("read_archive", { path: file.path });
    state.buffer = JSON.stringify(archive);
    state.readOnly = true;
  } else if (["pdf", "docx", "xlsx", "video"].includes(file.kind)) {
    const binary = await invoke<BinaryDocument>("read_binary", { path: file.path });
    state.buffer = binary.base64;
    state.readOnly = true;
  } else {
    const result = await invoke<TextReadResult>("read_text", { path: file.path });
    if (result.status !== "success") {
      state.status = "error";
      state.error = {
        code: result.status,
        title: result.status === "binary-content" ? "这是一个二进制文件" : "暂不支持此文本编码",
        message: result.message,
        recoverable: true,
      };
      return;
    }
    const text = result.document;
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

function openCurrentAsHex() {
  if (!documentState.file || !["binary-content", "unsupported-encoding"].includes(documentState.error?.code ?? "")) return;
  documentState.file.kind = "hex";
  documentState.viewerKind = "hex";
  documentState.buffer = null;
  documentState.readOnly = true;
  documentState.dirty = false;
  documentState.truncated = false;
  documentState.error = null;
  documentState.status = "ready";
  addRecent(documentState.file.path, documentState.file.name);
  syncActiveTab();
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
  persistTabs();
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
  tabResizeObserver = new ResizeObserver(updateTabScrollState);
  if (tabStrip.value) tabResizeObserver.observe(tabStrip.value);
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

watch(
  () => tabs.length,
  () => void nextTick(() => {
    updateTabScrollState();
    scrollActiveTabIntoView();
  }),
);

watch(tabStrip, (element, previousElement) => {
  if (previousElement) tabResizeObserver?.unobserve(previousElement);
  if (element) tabResizeObserver?.observe(element);
  updateTabScrollState();
});

onBeforeUnmount(() => {
  tabResizeObserver?.disconnect();
  disposeLifecycle?.();
});
</script>

<template>
  <AppShell :immersive="tabs.length > 0">
    <template #title-content>
      <div v-if="tabs.length" class="workspace-tab-viewport relative h-full max-w-full" data-tauri-drag-region>
        <div
          ref="tabStrip"
          class="workspace-tab-strip flex h-full items-center gap-1 overflow-x-auto"
          role="tablist"
          aria-label="已打开文件"
          @scroll="updateTabScrollState"
          @wheel="handleTabWheel"
        >
          <UContextMenu v-for="tab in tabs" :key="tab.id" :items="tabContextItems(tab)" :content="{ alignOffset: -4 }">
            <div
              :data-tab-id="tab.id"
              class="workspace-tab group flex h-7 max-w-52 shrink-0 items-center rounded-md border px-1 transition-colors"
              :class="tab.id === activeTabId ? 'border-primary/30 bg-primary/10 text-primary' : 'border-transparent text-muted hover:bg-accented hover:text-highlighted'"
              @contextmenu="activateTab(tab.id)"
            >
              <UTooltip :text="tab.state.file?.path ?? tab.fallbackName" :delay-duration="2000" :content="{ side: 'bottom', align: 'center', sideOffset: 6 }" :ui="{ content: 'max-w-md break-all font-mono' }">
                <button
                  class="flex h-full min-w-0 items-center gap-2 px-2 text-xs"
                  role="tab"
                  :aria-selected="tab.id === activeTabId"
                  :tabindex="tab.id === activeTabId ? 0 : -1"
                  @click="activateTab(tab.id)"
                >
                  <span v-if="tab.state.dirty" class="size-1.5 shrink-0 rounded-full bg-amber-500" />
                  <span class="truncate">{{ tab.state.file?.name ?? tab.fallbackName }}</span>
                </button>
              </UTooltip>
              <UTooltip text="关闭标签页" :delay-duration="500"><UButton class="workspace-tab-close" color="neutral" variant="ghost" size="xs" icon="i-lucide-x" aria-label="关闭标签页" @click.stop="closeTab(tab.id)" /></UTooltip>
            </div>
          </UContextMenu>
        </div>
        <div v-if="canScrollTabsLeft" class="workspace-tab-fade workspace-tab-fade-left" aria-hidden="true" />
        <div v-if="canScrollTabsRight" class="workspace-tab-fade workspace-tab-fade-right" aria-hidden="true" />
      </div>
    </template>

    <template #actions>
      <UTooltip text="打开文件" :delay-duration="500"><UButton class="titlebar-action" color="neutral" variant="ghost" size="xs" icon="i-lucide-folder-open" aria-label="打开文件" @click="selectFile" /></UTooltip>
      <UTooltip v-if="documentState.file" :text="currentFavorite ? '取消收藏' : '收藏文件'" :delay-duration="500"><UButton class="titlebar-action" :class="{ 'titlebar-favorite-active': currentFavorite }" color="neutral" variant="ghost" size="xs" :icon="currentFavorite ? 'i-mdi-star' : 'i-lucide-star'" :aria-label="currentFavorite ? '取消收藏' : '收藏文件'" @click="toggleCurrentFavorite" /></UTooltip>
      <UTooltip text="文件库" :delay-duration="500"><UButton class="titlebar-action" color="neutral" variant="ghost" size="xs" icon="i-lucide-library" aria-label="文件库" @click="libraryOpen = true" /></UTooltip>
      <UTooltip text="设置" :delay-duration="500"><UButton class="titlebar-action" color="neutral" variant="ghost" size="xs" icon="i-lucide-settings" aria-label="设置" @click="settingsOpen = true" /></UTooltip>
    </template>

    <div v-if="tabs.length" class="flex size-full min-h-0 flex-col">
      <div v-if="documentState.error" class="flex min-h-0 flex-1 flex-col">
        <div class="grid min-h-0 flex-1 place-items-center p-6">
          <ErrorState
            :error="documentState.error"
            :can-open-hex="['binary-content', 'unsupported-encoding'].includes(documentState.error.code)"
            @dismiss="closeTab()"
            @open-hex="openCurrentAsHex"
          />
        </div>
        <ViewerFooter><template #status>{{ (documentState.file?.extension || "未知格式").toUpperCase() }} · 打开失败</template></ViewerFooter>
      </div>
      <div v-else-if="documentState.file && documentState.status === 'ready' && hasViewer" class="flex min-h-0 flex-1 flex-col">
        <UAlert v-if="documentState.truncated" class="m-3 shrink-0" color="warning" variant="subtle" icon="i-lucide-file-warning" title="大文件只读预览" description="文件超过 20 MiB，仅加载前 2 MiB 内容以保持界面流畅。" />
        <KeepAlive :max="12">
          <ViewerHost :key="activeTabId" class="min-h-0 flex-1" :file="documentState.file" :content="documentState.buffer ?? ''" :read-only="documentState.readOnly" @update:content="updateContent" />
        </KeepAlive>
      </div>
      <div v-else-if="documentState.file && documentState.status === 'ready'" class="flex min-h-0 flex-1 flex-col">
        <div class="grid min-h-0 flex-1 place-items-center p-6"><FileReadyState :file="documentState.file" @open="selectFile" /></div>
        <ViewerFooter><template #status>{{ (documentState.file.extension || "未知格式").toUpperCase() }} · 暂无查看器</template></ViewerFooter>
      </div>
      <div v-else-if="documentState.status === 'loading'" class="flex min-h-0 flex-1 flex-col">
        <div class="grid flex-1 place-items-center text-center">
          <div><UIcon name="i-lucide-loader-circle" class="mx-auto size-8 animate-spin text-primary" /><p class="mt-3 text-sm text-muted">正在加载文件…</p></div>
        </div>
        <ViewerFooter><template #status>正在加载</template></ViewerFooter>
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
