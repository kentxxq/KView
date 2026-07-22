<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { defineShortcuts, useToast } from "@nuxt/ui/composables";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import AppShell from "./AppShell.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import FileLibraryModal from "./FileLibraryModal.vue";
import FileReadyState from "./FileReadyState.vue";
import OpenUrlModal from "./OpenUrlModal.vue";
import SettingsModal from "./SettingsModal.vue";
import ViewerFooter from "./ViewerFooter.vue";
import ViewerHost from "../viewers/ViewerHost.vue";
import { resolveViewer, resolveViewerKey } from "../viewers/registry";
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
  InstallerDocument,
  OpenedFile,
  RemoteDocument,
  SaveResult,
  TextReadResult,
} from "../types/document";
import { initializeDesktopLifecycle } from "../services/desktop-lifecycle";
import { recordDiagnostic } from "../services/diagnostics";
import { appSettings, loadSettings } from "../state/settings";

interface WorkspaceTab {
  id: string;
  sourceKey: string;
  state: DocumentState;
  fallbackName: string;
}

interface SiblingFileList {
  paths: string[];
  currentIndex: number;
}

const toast = useToast();
const tabs = reactive<WorkspaceTab[]>([]);
const activeTabId = ref("");
const spaceOpenTabId = ref("");
const spaceOpenPaths = ref<string[]>([]);
const spaceOpenIndex = ref(-1);
const dragActive = ref(false);
const guardOpen = ref(false);
const settingsOpen = ref(false);
const libraryOpen = ref(false);
const openUrlModalOpen = ref(false);
const guardTargetIds = ref<string[]>([]);
const tabStrip = ref<HTMLElement>();
const canScrollTabsLeft = ref(false);
const canScrollTabsRight = ref(false);
let tabSequence = 0;
let spaceOpenSequence = 0;
let spaceOpenSessionSequence = 0;
let disposeLifecycle: (() => void) | undefined;
let resolveGuard: ((result: boolean) => void) | undefined;
let tabResizeObserver: ResizeObserver | undefined;

const activeTab = computed(() => tabs.find((tab) => tab.id === activeTabId.value) ?? null);
const activeViewerKey = computed(() => documentState.file
  ? resolveViewerKey(documentState.file) ?? documentState.viewerKind
  : documentState.viewerKind);
const guardedDirtyTabs = computed(() => {
  const targetIds = new Set(guardTargetIds.value);
  return tabs.filter((tab) => targetIds.has(tab.id) && tab.state.dirty);
});
const hasViewer = computed(() => !!documentState.file && !!resolveViewer(documentState.file));
const currentFavorite = computed(() => {
  const file = documentState.file;
  return file?.source === "local" && isFavorite(file.path);
});

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function syncActiveTab() {
  if (activeTab.value) Object.assign(activeTab.value.state, documentState);
  persistTabs();
}

function persistTabs() {
  const activeFile = activeTab.value?.state.file;
  const activeLocalPath = activeFile?.source === "local" ? activeFile.path : null;
  saveSession(
    tabs.flatMap((tab) => {
      const file = tab.state.file;
      return file?.source === "local" ? [file.path] : [];
    }),
    activeLocalPath,
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
  const file = tab.state.file;
  const path = file?.source === "local" ? file.path : null;
  const index = tabs.findIndex((item) => item.id === tab.id);
  const closeItems = [
    {
      label: "关闭右侧标签页",
      disabled: index < 0 || index === tabs.length - 1,
      onSelect: () => closeTabsToRight(tab.id),
    },
    {
      label: "关闭其他标签页",
      disabled: tabs.length < 2,
      onSelect: () => closeOtherTabs(tab.id),
    },
    {
      label: "关闭所有标签页",
      onSelect: () => closeAllTabs(tab.id),
    },
  ];
  if (!path) return closeItems;
  return [
    {
      label: "打开文件位置",
      icon: "i-lucide-folder-open",
      onSelect: () => revealFile(path),
    },
    {
      label: "复制路径",
      icon: "i-lucide-copy",
      onSelect: () => copyFilePath(path),
    },
    { type: "separator" as const },
    {
      label: "打开方式",
      icon: "i-lucide-external-link",
      onSelect: () => openFileWith(path),
    },
    { type: "separator" as const },
    ...closeItems,
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

async function loadFileState(
  state: DocumentState,
  path: string,
  options: { inspectedFile?: OpenedFile } = {},
) {
  const file = options.inspectedFile ?? await invoke<OpenedFile>("inspect_file", { path });
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
  } else if (file.kind === "installer") {
    const installer = await invoke<InstallerDocument>("read_installer", { path: file.path });
    state.buffer = JSON.stringify(installer);
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

async function loadSpaceOpenPath(path: string | null) {
  const sequence = ++spaceOpenSequence;
  let tab = tabs.find((item) => item.id === spaceOpenTabId.value);
  if (!tab) {
    syncActiveTab();
    tab = reactive<WorkspaceTab>({
      id: `space-open-${Date.now()}-${tabSequence++}`,
      sourceKey: "space-open:",
      state: createDocumentState(),
      fallbackName: "空格打开",
    });
    tabs.push(tab);
    spaceOpenTabId.value = tab.id;
  }
  tab.sourceKey = path ? `local:${path}` : "space-open:unavailable";
  tab.fallbackName = path?.split(/[\\/]/).pop() || "空格打开";
  if (activeTabId.value === tab.id) void nextTick(scrollActiveTabIntoView);
  else activateTab(tab.id);
  if (!path) {
    Object.assign(tab.state, createDocumentState(), { status: "error" });
    tab.state.error = {
      code: "space-open-unavailable",
      title: "当前项目无法打开",
      message: "请选择一个文件，然后继续使用方向键切换打开。",
      recoverable: false,
    };
    Object.assign(documentState, tab.state);
    persistTabs();
    return;
  }
  if (tab.state.status === "empty") {
    tab.state.status = "loading";
    Object.assign(documentState, tab.state);
  }

  const nextState = createDocumentState();
  nextState.status = "loading";
  try {
    const file = await invoke<OpenedFile>("inspect_file", { path });
    const nextViewer = resolveViewer(file);
    if (!nextViewer) throw new Error(`暂不支持 .${file.extension || "未知"} 文件`);
    if (sequence !== spaceOpenSequence || spaceOpenTabId.value !== tab.id) return;

    const currentViewerKey = tab.state.file ? resolveViewerKey(tab.state.file) : null;
    const nextViewerKey = nextViewer.componentKey ?? nextViewer.kind;
    const canReuseViewer = tab.state.status === "ready" && currentViewerKey === nextViewerKey;
    if (!canReuseViewer) {
      Object.assign(tab.state, createDocumentState(), {
        file,
        status: "loading",
        viewerKind: file.kind,
      });
      if (activeTabId.value === tab.id) Object.assign(documentState, tab.state);
    }

    await loadFileState(nextState, path, { inspectedFile: file });
  } catch (error) {
    nextState.status = "error";
    nextState.error = {
      code: "space-open-failed",
      title: "无法打开文件",
      message: errorMessage(error),
      recoverable: true,
    };
  }
  if (sequence === spaceOpenSequence && spaceOpenTabId.value === tab.id) {
    Object.assign(tab.state, nextState);
    tab.sourceKey = `local:${path}`;
    tab.fallbackName = nextState.file?.name ?? tab.fallbackName;
    if (activeTabId.value === tab.id) Object.assign(documentState, tab.state);
    persistTabs();
  }
}

async function loadSpaceOpenDirectory(path: string, session: number) {
  try {
    const directory = await invoke<SiblingFileList>("list_sibling_files", { path });
    if (session !== spaceOpenSessionSequence || !spaceOpenTabId.value) return;
    if (!directory.paths.length || directory.currentIndex < 0 || directory.currentIndex >= directory.paths.length) {
      throw new Error("文件列表为空或当前文件索引无效");
    }
    spaceOpenPaths.value = directory.paths;
    spaceOpenIndex.value = directory.currentIndex;
  } catch (error) {
    if (session !== spaceOpenSessionSequence || !spaceOpenTabId.value) return;
    spaceOpenPaths.value = [path];
    spaceOpenIndex.value = 0;
    recordDiagnostic("space-open", "directory-list-failed");
    toast.add({
      id: "space-open-directory-list-failed",
      title: "无法读取同目录文件",
      description: `${errorMessage(error)}，方向键切换暂不可用。`,
      color: "warning",
      icon: "i-lucide-folder-x",
    });
  }
}

async function startSpaceOpen(path: string | null) {
  const session = ++spaceOpenSessionSequence;
  spaceOpenPaths.value = path ? [path] : [];
  spaceOpenIndex.value = path ? 0 : -1;
  const fileLoad = loadSpaceOpenPath(path);
  if (path) void loadSpaceOpenDirectory(path, session);
  await fileLoad;
}

function switchSpaceOpenFile(direction: number) {
  const tab = activeTab.value;
  if (!tab || tab.id !== spaceOpenTabId.value || spaceOpenPaths.value.length < 2) return;
  if (tab.state.dirty) {
    toast.add({
      id: "space-open-unsaved-changes",
      title: "当前文件有未保存修改",
      description: "请先保存当前文件，再使用方向键切换。",
      color: "warning",
      icon: "i-lucide-file-warning",
    });
    return;
  }

  const length = spaceOpenPaths.value.length;
  const currentIndex = spaceOpenIndex.value >= 0 && spaceOpenIndex.value < length
    ? spaceOpenIndex.value
    : 0;
  const nextIndex = (currentIndex + direction + length) % length;
  spaceOpenIndex.value = nextIndex;
  void loadSpaceOpenPath(spaceOpenPaths.value[nextIndex]);
}

function resetSpaceOpenLink() {
  spaceOpenSequence++;
  spaceOpenSessionSequence++;
  spaceOpenTabId.value = "";
  spaceOpenPaths.value = [];
  spaceOpenIndex.value = -1;
}

async function cancelSpaceOpenLink() {
  const tabId = spaceOpenTabId.value;
  if (!tabId) return;
  await invoke("cancel_space_open");
  if (spaceOpenTabId.value === tabId) resetSpaceOpenLink();
}

function handleSpaceOpenCloseRequested() {
  resetSpaceOpenLink();
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
  const sourceKey = `local:${path}`;
  const existing = tabs.find((tab) => tab.sourceKey === sourceKey);
  if (existing) {
    activateTab(existing.id);
    if (existing.state.file) addRecent(existing.state.file.path, existing.state.file.name);
    return;
  }

  const state = createDocumentState();
  state.status = "loading";
  const tab: WorkspaceTab = {
    id: `tab-${Date.now()}-${tabSequence++}`,
    sourceKey,
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

async function openRemoteAsTab(url: string) {
  const sourceKey = `remote:${url}`;
  const existing = tabs.find((tab) => tab.sourceKey === sourceKey);
  if (existing) {
    activateTab(existing.id);
    return;
  }

  const state = createDocumentState();
  state.status = "loading";
  const parsed = new URL(url);
  const tab: WorkspaceTab = {
    id: `tab-${Date.now()}-${tabSequence++}`,
    sourceKey,
    state,
    fallbackName: parsed.pathname.split("/").filter(Boolean).pop() || parsed.hostname,
  };
  tabs.push(tab);
  activateTab(tab.id);
  const startedAt = performance.now();
  try {
    const remote = await invoke<RemoteDocument>("fetch_remote_document", { url });
    if (!resolveViewer(remote.file)) throw new Error("远程内容没有可用的查看器");
    state.file = remote.file;
    state.viewerKind = remote.file.kind;
    state.sourceModifiedAt = null;
    state.buffer = remote.content;
    state.readOnly = true;
    state.dirty = false;
    state.truncated = false;
    state.status = "ready";
    recordDiagnostic("performance", `open-remote-${state.viewerKind}`, performance.now() - startedAt);
  } catch (error) {
    recordDiagnostic("viewer", "open-remote-failed", performance.now() - startedAt);
    state.status = "error";
    state.error = {
      code: "open-remote-failed",
      title: "无法打开 URL",
      message: errorMessage(error),
      recoverable: true,
    };
  }
  if (activeTabId.value === tab.id) Object.assign(documentState, state);
  persistTabs();
}

function readFileAsDataUrl(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => typeof reader.result === "string"
      ? resolve(reader.result)
      : reject(new Error("无法读取剪贴板图片"));
    reader.onerror = () => reject(reader.error ?? new Error("无法读取剪贴板图片"));
    reader.readAsDataURL(file);
  });
}

async function openClipboardImage(file: File) {
  const extension = ({
    "image/jpeg": "jpg",
    "image/png": "png",
    "image/webp": "webp",
    "image/gif": "gif",
    "image/svg+xml": "svg",
  } as Record<string, string>)[file.type.toLowerCase()];
  if (!extension) {
    toast.add({ title: "不支持此剪贴板图片格式", description: file.type || "未知格式", color: "error", icon: "i-lucide-image-off" });
    return;
  }
  if (file.size > 100 * 1024 * 1024) {
    toast.add({ title: "剪贴板图片过大", description: "图片超过 100 MiB 安全限制。", color: "error", icon: "i-lucide-image-off" });
    return;
  }

  const sequence = tabSequence++;
  const id = `tab-${Date.now()}-${sequence}`;
  const state = createDocumentState();
  state.status = "loading";
  const tab: WorkspaceTab = {
    id,
    sourceKey: `clipboard:${id}`,
    state,
    fallbackName: `粘贴的图片.${extension}`,
  };
  tabs.push(tab);
  activateTab(tab.id);
  try {
    state.file = {
      source: "clipboard",
      path: `clipboard://image/${id}`,
      name: file.name || `粘贴的图片.${extension}`,
      extension,
      size: file.size,
      modifiedAt: Date.now(),
      kind: "image",
      encoding: null,
      lineEnding: null,
      readOnly: true,
    };
    state.viewerKind = "image";
    state.buffer = await readFileAsDataUrl(file);
    state.readOnly = true;
    state.status = "ready";
    recordDiagnostic("viewer", "open-clipboard-image");
  } catch (error) {
    state.status = "error";
    state.error = {
      code: "open-clipboard-image-failed",
      title: "无法打开剪贴板图片",
      message: errorMessage(error),
      recoverable: true,
    };
  }
  if (activeTabId.value === tab.id) Object.assign(documentState, state);
  persistTabs();
}

function openClipboardText(text: string) {
  const sequence = tabSequence++;
  const id = `tab-${Date.now()}-${sequence}`;
  const name = "粘贴的文本.txt";
  const state = createDocumentState();
  state.file = {
    source: "clipboard",
    path: `clipboard://text/${id}`,
    name,
    extension: "txt",
    size: new Blob([text]).size,
    modifiedAt: Date.now(),
    kind: "text",
    encoding: "utf-8",
    lineEnding: text.includes("\r\n") ? "crlf" : text.includes("\r") ? "cr" : "lf",
    readOnly: false,
  };
  state.status = "ready";
  state.viewerKind = "text";
  state.buffer = text;
  state.dirty = true;
  state.encoding = "utf-8";
  state.lineEnding = state.file.lineEnding;
  state.readOnly = false;
  const tab: WorkspaceTab = {
    id,
    sourceKey: `clipboard:${id}`,
    state,
    fallbackName: name,
  };
  tabs.push(tab);
  activateTab(id);
  Object.assign(documentState, state);
  persistTabs();
  recordDiagnostic("viewer", "open-clipboard-text");
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
  const shouldSaveAs = saveAs || state.file.source === "clipboard";
  let targetPath = state.file.path;
  if (shouldSaveAs) {
    const selected = await save({
      defaultPath: state.file.source === "clipboard" ? state.file.name : state.file.path,
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
      expectedModifiedAt: shouldSaveAs ? null : state.sourceModifiedAt,
    });
    const refreshed = await invoke<OpenedFile>("inspect_file", { path: result.path });
    refreshed.encoding = state.encoding;
    refreshed.lineEnding = state.lineEnding;
    refreshed.readOnly = false;
    state.file = refreshed;
    state.sourceModifiedAt = result.modifiedAt;
    state.dirty = false;
    addRecent(refreshed.path, refreshed.name);
    toast.add({ title: shouldSaveAs ? "文件已另存" : "文件已保存", description: result.name, color: "success", icon: "i-lucide-check" });
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

async function requestDirtyDecision(targetTabs: WorkspaceTab[]) {
  syncActiveTab();
  const dirtyTargetIds = targetTabs
    .filter((tab) => tab.state.dirty)
    .map((tab) => tab.id);
  if (!dirtyTargetIds.length) return true;
  if (resolveGuard) return false;
  guardTargetIds.value = dirtyTargetIds;
  guardOpen.value = true;
  return new Promise<boolean>((resolve) => { resolveGuard = resolve; });
}

function finishGuard(result: boolean) {
  guardOpen.value = false;
  resolveGuard?.(result);
  resolveGuard = undefined;
  guardTargetIds.value = [];
}

function handleGuardOpenChange(open: boolean) {
  if (!open && resolveGuard) finishGuard(false);
}

async function saveAllFromGuard() {
  for (const tab of [...guardedDirtyTabs.value]) {
    if (!(await saveState(tab.state, false))) return;
  }
  if (activeTab.value) Object.assign(documentState, activeTab.value.state);
  finishGuard(true);
}

async function closeTabs(targetIds: string[]) {
  const targetIdSet = new Set(targetIds);
  const targetTabs = tabs.filter((tab) => targetIdSet.has(tab.id));
  if (!targetTabs.length || !(await requestDirtyDecision(targetTabs))) return;

  const firstTargetIndex = tabs.findIndex((tab) => targetIdSet.has(tab.id));
  const activeTabClosed = targetIdSet.has(activeTabId.value);
  if (spaceOpenTabId.value && targetIdSet.has(spaceOpenTabId.value)) {
    await cancelSpaceOpenLink();
  }
  for (let index = tabs.length - 1; index >= 0; index--) {
    if (targetIdSet.has(tabs[index].id)) tabs.splice(index, 1);
  }

  if (activeTabClosed) {
    activeTabId.value = "";
    const nextTab = tabs[Math.min(firstTargetIndex, tabs.length - 1)];
    if (nextTab) activateTab(nextTab.id);
    else resetDocument();
  }
  persistTabs();
}

async function closeTab(id = activeTabId.value) {
  if (!tabs.some((tab) => tab.id === id)) return;
  activateTab(id);
  await closeTabs([id]);
}

async function closeTabsToRight(id: string) {
  const index = tabs.findIndex((tab) => tab.id === id);
  if (index < 0) return;
  activateTab(id);
  await closeTabs(tabs.slice(index + 1).map((tab) => tab.id));
}

async function closeOtherTabs(id: string) {
  if (!tabs.some((tab) => tab.id === id)) return;
  activateTab(id);
  await closeTabs(tabs.filter((tab) => tab.id !== id).map((tab) => tab.id));
}

async function closeAllTabs(id: string) {
  if (tabs.some((tab) => tab.id === id)) activateTab(id);
  await closeTabs(tabs.map((tab) => tab.id));
}

function cycleTab(direction: number) {
  if (tabs.length < 2) return;
  const index = tabs.findIndex((tab) => tab.id === activeTabId.value);
  activateTab(tabs[(index + direction + tabs.length) % tabs.length].id);
}

function toggleCurrentFavorite() {
  if (!documentState.file || documentState.file.source !== "local") return;
  toggleFavorite(documentState.file.path, documentState.file.name);
}

function clearWorkspace() {
  tabs.splice(0);
  activeTabId.value = "";
  resetDocument();
  persistTabs();
}

async function returnHome() {
  if (!tabs.length) return;
  if (!(await requestDirtyDecision([...tabs]))) return;
  await cancelSpaceOpenLink();
  clearWorkspace();
}

async function closeWindow() {
  if (!isTauri()) return;
  await getCurrentWindow().close();
}

function keepTabsOnWindowClose() {
  return appSettings.residencyMode === "fast" && appSettings.closeBehavior === "keep-tabs";
}

const canSwitchSpaceOpen = computed(() => (
  activeTabId.value === spaceOpenTabId.value
  && spaceOpenPaths.value.length > 1
  && !guardOpen.value
  && !settingsOpen.value
  && !libraryOpen.value
  && !openUrlModalOpen.value
));

defineShortcuts(computed(() => ({
  escape: { handler: closeWindow, usingInput: true },
  meta_o: selectFile,
  meta_s: () => saveDocument(false),
  meta_shift_s: () => saveDocument(true),
  meta_w: () => closeTab(),
  meta_tab: () => cycleTab(1),
  meta_shift_tab: () => cycleTab(-1),
  arrowleft: canSwitchSpaceOpen.value ? () => switchSpaceOpenFile(-1) : false,
  arrowup: canSwitchSpaceOpen.value ? () => switchSpaceOpenFile(-1) : false,
  arrowright: canSwitchSpaceOpen.value ? () => switchSpaceOpenFile(1) : false,
  arrowdown: canSwitchSpaceOpen.value ? () => switchSpaceOpenFile(1) : false,
})));

onMounted(async () => {
  tabResizeObserver = new ResizeObserver(updateTabScrollState);
  if (tabStrip.value) tabResizeObserver.observe(tabStrip.value);
  if (!isTauri()) return;
  try {
    await Promise.all([loadSettings(), loadLibrary()]);
  } catch {
    recordDiagnostic("settings", "load-failed");
  }
  if (!tabs.length && libraryState.sessionPaths.length) {
    await openPaths(libraryState.sessionPaths);
    const restored = tabs.find((tab) => tab.state.file?.path === libraryState.activePath);
    if (restored) activateTab(restored.id);
  }
  disposeLifecycle = await initializeDesktopLifecycle({
    onPaths: openPaths,
    onSpaceOpenPath: startSpaceOpen,
    onSpaceOpenCloseRequested: handleSpaceOpenCloseRequested,
    onRequestClose: () => keepTabsOnWindowClose() || requestDirtyDecision([...tabs]),
    onBeforeClose: async () => {
      await cancelSpaceOpenLink();
      if (!keepTabsOnWindowClose()) clearWorkspace();
    },
    onDragStateChange: (active) => { dragActive.value = active; },
    onOpenFile: selectFile,
    onCloseTab: () => closeTab(),
    onNextTab: () => cycleTab(1),
    onPreviousTab: () => cycleTab(-1),
  });
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
      <UTooltip text="回到首页" :delay-duration="500"><UButton class="titlebar-action" color="neutral" variant="ghost" size="xs" icon="i-lucide-house" aria-label="回到首页" :disabled="!tabs.length" @click="returnHome" /></UTooltip>
      <UTooltip text="打开文件" :delay-duration="500"><UButton class="titlebar-action" color="neutral" variant="ghost" size="xs" icon="i-lucide-folder-open" aria-label="打开文件" @click="selectFile" /></UTooltip>
      <UTooltip text="打开 URL" :delay-duration="500"><UButton class="titlebar-action" color="neutral" variant="ghost" size="xs" icon="i-lucide-link" aria-label="打开 URL" @click="openUrlModalOpen = true" /></UTooltip>
      <UTooltip v-if="documentState.file?.source === 'local'" :text="currentFavorite ? '取消收藏' : '收藏文件'" :delay-duration="500"><UButton class="titlebar-action" :class="{ 'titlebar-favorite-active': currentFavorite }" color="neutral" variant="ghost" size="xs" :icon="currentFavorite ? 'i-mdi-star' : 'i-lucide-star'" :aria-label="currentFavorite ? '取消收藏' : '收藏文件'" @click="toggleCurrentFavorite" /></UTooltip>
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
          <ViewerHost :key="activeTab?.id === spaceOpenTabId ? `space-open:${activeViewerKey}` : activeTabId" class="min-h-0 flex-1" :file="documentState.file" :content="documentState.buffer ?? ''" :read-only="documentState.readOnly" @update:content="updateContent" />
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
    <EmptyState
      v-else
      :drag-active="dragActive"
      @select="selectFile"
      @open-url="openUrlModalOpen = true"
      @paste-url="openRemoteAsTab"
      @paste-image="openClipboardImage"
      @paste-text="openClipboardText"
    />

    <UModal :open="guardOpen" :dismissible="false" title="保存修改？" @update:open="handleGuardOpenChange">
      <template #body><p class="text-sm leading-6 text-muted">本次操作涉及的标签页中，共有 {{ guardedDirtyTabs.length }} 个包含未保存修改。保存后继续，或放弃这些修改。</p></template>
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <UButton color="neutral" variant="ghost" label="取消" @click="finishGuard(false)" />
          <UButton color="error" variant="soft" label="放弃修改并继续" @click="finishGuard(true)" />
          <UButton icon="i-lucide-save" label="保存全部" @click="saveAllFromGuard" />
        </div>
      </template>
    </UModal>
    <FileLibraryModal v-model:open="libraryOpen" @open="openPathAsTab" @favorite="toggleFavorite" @remove-recent="removeRecent" />
    <OpenUrlModal v-model:open="openUrlModalOpen" @open-url="openRemoteAsTab" />
    <SettingsModal v-model:open="settingsOpen" />
  </AppShell>
</template>
