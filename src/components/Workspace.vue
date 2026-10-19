<script setup lang="ts">
import {
  computed,
  defineComponent,
  nextTick,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  toRef,
  watch,
  type Component,
} from "vue";
import { defineShortcuts, useToast } from "@nuxt/ui/composables";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import AppShell from "./AppShell.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import FileLibraryModal from "./FileLibraryModal.vue";
import OpenUrlModal from "./OpenUrlModal.vue";
import SettingsModal from "./SettingsModal.vue";
import ViewerFooter from "./ViewerFooter.vue";
import ViewerHost from "../viewers/ViewerHost.vue";
import { resolveViewer, resolveViewerKey } from "../viewers/registry";
import { createDocumentState } from "../state/document";
import { useLibraryStore } from "../state/library";
import { useWorkspaceStore, type WorkspaceTab } from "../state/workspace";
import type { DocumentState, OpenedFile, RemoteDocument, SaveResult } from "../types/document";
import { decodeBase64 } from "../utils/binary";
import { loadLocalDocument } from "../services/document-loader";
import { initializeDesktopLifecycle } from "../services/desktop-lifecycle";
import { recordDiagnostic } from "../services/diagnostics";
import { useSettingsStore } from "../state/settings";
import { useUpdatesStore } from "../state/updates";
import { desktopInvoke as invoke } from "../services/desktop-api";
import {
  disposeDocumentResources as closeDocumentResources,
  transferDocumentResources,
} from "../services/document-resources";

interface SiblingFileList {
  paths: string[];
  currentIndex: number;
}

const toast = useToast();
const workspaceState = useWorkspaceStore();
const libraryState = useLibraryStore();
const appSettings = useSettingsStore();
const updates = useUpdatesStore();
const settingsInitialSection = ref<"general" | "about">("general");
const updatePending = ref(false);
const updateLocked = ref(false);
let disposeUpdates: (() => void) | undefined;
let disposeUpdateFocus: (() => void) | undefined;
let notificationPending = false;
let workspaceOperations = 0;

async function notifyUpdate() {
  const { phase, version, dismissedVersion, notes } = updates.status;
  if (!isTauri() || !version || version === dismissedVersion || notificationPending || !["available", "ready"].includes(phase)) return;
  notificationPending = true;
  try {
    const window = getCurrentWindow();
    if (!(await window.isVisible()) || await window.isMinimized()) return;
    await updates.dismiss();
    toast.add({
      id: "app-update", title: `KView ${version} 可用`,
      description: notes?.slice(0, 300) || "可前往设置查看更新并下载安装。",
      duration: 0, icon: "i-lucide-download",
      actions: [{ label: "查看更新", onClick: () => {
        settingsInitialSection.value = "about";
        settingsOpen.value = true;
      } }],
    });
  } catch { recordDiagnostic("updater", "notification-failed"); }
  finally { notificationPending = false; }
}

async function requestUpdate() {
  if (!isTauri() || updatePending.value || updates.busy) return;
  updatePending.value = true;
  try {
    await updates.download();
    // 不允许在已有文件载入或关闭流程期间进入安装保护。
    if (workspaceOperations || windowClosePending || resolveGuard || tabs.some((tab) => tab.state.status === "loading")) {
      toast.add({ title: "更新已下载", description: "请等待当前文件操作完成后，再点击安装并重启。", color: "warning" });
      return;
    }
    updateLocked.value = true;
    settingsOpen.value = false;
    libraryOpen.value = false;
    openUrlModalOpen.value = false;
    await nextTick();
    if (!(await requestDirtyDecision([...tabs]))) return;
    await cancelSpaceOpenLink();
    persistTabs();
    await invoke("install_update");
  } catch (error) {
    toast.add({ title: "更新未完成", description: errorMessage(error), color: "error" });
  } finally {
    updateLocked.value = false;
    updatePending.value = false;
  }
}

watch(() => updates.status.revision, () => { void notifyUpdate(); });
const addRecent = (path: string, name: string) => libraryState.addRecent(path, name);
const removeRecent = (path: string) => libraryState.removeRecent(path);
const toggleFavorite = (path: string, name: string) => libraryState.toggleFavorite(path, name);
const tabs = workspaceState.tabs;
const activeTabId = toRef(workspaceState, "activeTabId");
const emptyDocumentState = reactive(createDocumentState());
const spaceOpenTabId = ref("");
const spaceOpenPaths = ref<string[]>([]);
const spaceOpenIndex = ref(-1);
const dragActive = ref(false);
const guardOpen = ref(false);
const guardSaving = ref(false);
const settingsOpen = ref(false);
const libraryOpen = ref(false);
const openUrlModalOpen = ref(false);
const guardTargetIds = ref<string[]>([]);
const viewerCacheExclusions = ref<string[]>([]);
const viewerShells = new Map<string, Component>();
const tabStrip = ref<HTMLElement>();
const canScrollTabsLeft = ref(false);
const canScrollTabsRight = ref(false);
let tabSequence = 0;
let spaceOpenSequence = 0;
let spaceOpenSessionSequence = 0;
let windowClosePending = false;
let disposeLifecycle: (() => void) | undefined;
let resolveGuard: ((result: boolean) => void) | undefined;
let tabResizeObserver: ResizeObserver | undefined;

const activeTab = computed(() => tabs.find((tab) => tab.id === activeTabId.value) ?? null);
function viewerShellName(tabId: string) {
  return `KViewTabViewer_${tabId.replace(/[^a-zA-Z0-9_]/g, "_")}`;
}

function viewerShellFor(tabId: string) {
  let shell = viewerShells.get(tabId);
  if (!shell) {
    shell = defineComponent({
      name: viewerShellName(tabId),
      setup(_, { slots }) {
        return () => slots.default?.();
      },
    });
    viewerShells.set(tabId, shell);
  }
  return shell;
}

const activeViewerShell = computed(() =>
  activeTab.value ? viewerShellFor(activeTab.value.id) : null,
);
const documentState = computed(() => activeTab.value?.state ?? emptyDocumentState);
const activeViewerKey = computed(() =>
  documentState.value.file
    ? (resolveViewerKey(documentState.value.file) ?? documentState.value.viewerKind)
    : documentState.value.viewerKind,
);
const guardedDirtyTabs = computed(() => {
  const targetIds = new Set(guardTargetIds.value);
  return tabs.filter((tab) => targetIds.has(tab.id) && tab.state.dirty);
});
const hasViewer = computed(
  () => !!documentState.value.file && !!resolveViewer(documentState.value.file),
);
const currentFavorite = computed(() => {
  const file = documentState.value.file;
  return file?.source === "local" && libraryState.isFavorite(file.path);
});

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function syncActiveTab() {
  persistTabs();
}

function persistTabs() {
  const activeFile = activeTab.value?.state.file;
  const activeLocalPath = activeFile?.source === "local" ? activeFile.path : null;
  libraryState.saveSession(
    tabs.flatMap((tab) => {
      const file = tab.state.file;
      return file?.source === "local" ? [file.path] : [];
    }),
    activeLocalPath,
  );
}

function activateTab(id: string) {
  if (updateLocked.value) return;
  if (id === activeTabId.value) return;
  syncActiveTab();
  const target = tabs.find((tab) => tab.id === id);
  if (!target) return;
  activeTabId.value = id;
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
  element.scrollLeft +=
    Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
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
    toast.add({
      title: "无法打开文件位置",
      description: errorMessage(error),
      color: "error",
      icon: "i-lucide-triangle-alert",
    });
  }
}

async function copyFilePath(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    toast.add({ title: "路径已复制", description: path, color: "success", icon: "i-lucide-copy" });
  } catch (error) {
    toast.add({
      title: "无法复制路径",
      description: errorMessage(error),
      color: "error",
      icon: "i-lucide-triangle-alert",
    });
  }
}

async function openFileWith(path: string) {
  try {
    await invoke("open_file_with", { path });
  } catch (error) {
    toast.add({
      title: "无法选择打开方式",
      description: errorMessage(error),
      color: "error",
      icon: "i-lucide-triangle-alert",
    });
  }
}

async function loadFileState(
  target: DocumentState,
  path: string,
  options: { inspectedFile?: OpenedFile } = {},
) {
  await loadLocalDocument(target, path, { ...options, onRecent: addRecent });
}

async function loadSpaceOpenPath(path: string | null) {
  if (updateLocked.value) return;
  workspaceOperations++;
  try { await loadSpaceOpenPathImpl(path); }
  finally { workspaceOperations--; }
}

async function loadSpaceOpenPathImpl(path: string | null) {
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
    await closeDocumentResources(tab.state);
    Object.assign(tab.state, createDocumentState(), { status: "error" });
    tab.state.error = {
      code: "space-open-unavailable",
      title: "当前项目无法打开",
      message: "请选择一个文件，然后继续使用方向键切换打开。",
      recoverable: false,
    };
    persistTabs();
    return;
  }
  if (tab.state.status === "empty") {
    tab.state.status = "loading";
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
      await closeDocumentResources(tab.state);
      Object.assign(tab.state, createDocumentState(), {
        file,
        status: "loading",
        viewerKind: file.kind,
      });
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
    await closeDocumentResources(tab.state);
    transferDocumentResources(nextState, tab.state);
    Object.assign(tab.state, nextState);
    tab.sourceKey = `local:${path}`;
    tab.fallbackName = nextState.file?.name ?? tab.fallbackName;
    persistTabs();
  } else {
    await closeDocumentResources(nextState);
  }
}

async function loadSpaceOpenDirectory(path: string, session: number) {
  try {
    const directory = await invoke<SiblingFileList>("list_sibling_files", { path });
    if (session !== spaceOpenSessionSequence || !spaceOpenTabId.value) return;
    if (
      !directory.paths.length ||
      directory.currentIndex < 0 ||
      directory.currentIndex >= directory.paths.length
    ) {
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
  if (updateLocked.value) return;
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
  const currentIndex =
    spaceOpenIndex.value >= 0 && spaceOpenIndex.value < length ? spaceOpenIndex.value : 0;
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
  await invoke("cancel_space_open");
  if (tabId && spaceOpenTabId.value === tabId) resetSpaceOpenLink();
}

async function openCurrentAsHex() {
  if (updateLocked.value) return;
  workspaceOperations++;
  try { await openCurrentAsHexImpl(); }
  finally { workspaceOperations--; }
}

async function openCurrentAsHexImpl() {
  if (
    !documentState.value.file ||
    (documentState.value.file.kind !== "ebook" &&
      !["binary-content", "unsupported-encoding"].includes(documentState.value.error?.code ?? ""))
  )
    return;
  await closeDocumentResources(documentState.value);
  documentState.value.file.kind = "hex";
  documentState.value.viewerKind = "hex";
  documentState.value.content = { type: "empty" };
  documentState.value.readOnly = true;
  documentState.value.dirty = false;
  documentState.value.largeTextSession = null;
  documentState.value.error = null;
  documentState.value.status = "ready";
  addRecent(documentState.value.file.path, documentState.value.file.name);
  syncActiveTab();
}

async function openPathAsTab(path: string) {
  if (updateLocked.value) return;
  const sourceKey = `local:${path}`;
  const existing = tabs.find((tab) => tab.sourceKey === sourceKey);
  if (existing) {
    activateTab(existing.id);
    if (existing.state.file) addRecent(existing.state.file.path, existing.state.file.name);
    return;
  }

  const state = reactive(createDocumentState());
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
  persistTabs();
}

async function openPaths(paths: string[]) {
  for (const path of [...new Set(paths.filter(Boolean))]) await openPathAsTab(path);
}

async function openRemoteAsTab(url: string) {
  if (updateLocked.value) return;
  const sourceKey = `remote:${url}`;
  const existing = tabs.find((tab) => tab.sourceKey === sourceKey);
  if (existing) {
    activateTab(existing.id);
    return;
  }

  const state = reactive(createDocumentState());
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
    state.content =
      remote.file.kind === "image"
        ? { type: "data-url", value: remote.content }
        : { type: "bytes", value: decodeBase64(remote.content) };
    state.readOnly = true;
    state.dirty = false;
    state.largeTextSession = null;
    state.status = "ready";
    recordDiagnostic(
      "performance",
      `open-remote-${state.viewerKind}`,
      performance.now() - startedAt,
    );
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
  persistTabs();
}

function readFileAsDataUrl(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () =>
      typeof reader.result === "string"
        ? resolve(reader.result)
        : reject(new Error("无法读取剪贴板图片"));
    reader.onerror = () => reject(reader.error ?? new Error("无法读取剪贴板图片"));
    reader.readAsDataURL(file);
  });
}

async function openClipboardImage(file: File) {
  if (updateLocked.value) return;
  const extension = (
    {
      "image/jpeg": "jpg",
      "image/png": "png",
      "image/webp": "webp",
      "image/gif": "gif",
      "image/svg+xml": "svg",
    } as Record<string, string>
  )[file.type.toLowerCase()];
  if (!extension) {
    toast.add({
      title: "不支持此剪贴板图片格式",
      description: file.type || "未知格式",
      color: "error",
      icon: "i-lucide-image-off",
    });
    return;
  }
  if (file.size > 100 * 1024 * 1024) {
    toast.add({
      title: "剪贴板图片过大",
      description: "图片超过 100 MiB 安全限制。",
      color: "error",
      icon: "i-lucide-image-off",
    });
    return;
  }

  const sequence = tabSequence++;
  const id = `tab-${Date.now()}-${sequence}`;
  const state = reactive(createDocumentState());
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
    state.content = { type: "data-url", value: await readFileAsDataUrl(file) };
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
  persistTabs();
}

function openClipboardText(text: string) {
  if (updateLocked.value) return;
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
  state.content = { type: "text", value: text };
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
  persistTabs();
  recordDiagnostic("viewer", "open-clipboard-text");
}

async function selectFile() {
  if (updateLocked.value) return;
  if (!isTauri()) {
    documentState.value.error = {
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
  if (updateLocked.value) return;
  if (
    documentState.value.readOnly ||
    documentState.value.content.type !== "text" ||
    documentState.value.content.value === value
  )
    return;
  documentState.value.content = { type: "text", value };
  documentState.value.dirty = true;
  syncActiveTab();
}

async function saveState(state: DocumentState, saveAs: boolean) {
  workspaceOperations++;
  try { return await saveStateImpl(state, saveAs); }
  finally { workspaceOperations--; }
}

async function saveStateImpl(state: DocumentState, saveAs: boolean) {
  if (!state.file || state.readOnly || state.content.type !== "text" || state.status !== "ready")
    return false;
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

  const duplicate = tabs.find(
    (tab) => tab.state !== state && tab.sourceKey === `local:${targetPath}`,
  );
  if (duplicate) {
    toast.add({
      title: "目标文件已在其他标签页打开",
      description: "请关闭已有标签页或选择其他保存位置。",
      color: "warning",
      icon: "i-lucide-files",
    });
    return false;
  }

  try {
    const result = await invoke<SaveResult>("save_text", {
      path: targetPath,
      content: state.content.value,
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
    const owner = tabs.find((tab) => tab.state === state);
    if (owner) {
      owner.sourceKey = `local:${refreshed.path}`;
      owner.fallbackName = refreshed.name;
    }
    addRecent(refreshed.path, refreshed.name);
    toast.add({
      title: shouldSaveAs ? "文件已另存" : "文件已保存",
      description: result.name,
      color: "success",
      icon: "i-lucide-check",
    });
    return true;
  } catch (error) {
    recordDiagnostic("storage", "save-failed");
    toast.add({
      title: "保存失败",
      description: errorMessage(error).replace(/^external-modified:\s*/, ""),
      color: "error",
      icon: "i-lucide-triangle-alert",
      duration: 8000,
    });
    return false;
  }
}

async function saveDocument(saveAs: boolean) {
  if (updateLocked.value) return false;
  const saved = await saveState(documentState.value, saveAs);
  syncActiveTab();
  return saved;
}

async function requestDirtyDecision(targetTabs: WorkspaceTab[]) {
  syncActiveTab();
  const dirtyTargetIds = targetTabs.filter((tab) => tab.state.dirty).map((tab) => tab.id);
  if (!dirtyTargetIds.length) return true;
  if (resolveGuard) return false;
  guardTargetIds.value = dirtyTargetIds;
  guardOpen.value = true;
  return new Promise<boolean>((resolve) => {
    resolveGuard = resolve;
  });
}

function finishGuard(result: boolean) {
  if (guardSaving.value) return;
  guardOpen.value = false;
  resolveGuard?.(result);
  resolveGuard = undefined;
  guardTargetIds.value = [];
}

function handleGuardOpenChange(open: boolean) {
  if (!open && resolveGuard) finishGuard(false);
}

async function saveAllFromGuard() {
  if (guardSaving.value) return;
  guardSaving.value = true;
  try {
    for (const tab of [...guardedDirtyTabs.value]) {
      if (!(await saveState(tab.state, false))) return;
    }
  } finally { guardSaving.value = false; }
  finishGuard(true);
}

async function closeTabs(targetIds: string[]) {
  if (updateLocked.value) return;
  workspaceOperations++;
  try { await closeTabsImpl(targetIds); }
  finally { workspaceOperations--; }
}

async function closeTabsImpl(targetIds: string[]) {
  const targetIdSet = new Set(targetIds);
  const targetTabs = tabs.filter((tab) => targetIdSet.has(tab.id));
  if (!targetTabs.length || !(await requestDirtyDecision(targetTabs))) return;

  const firstTargetIndex = tabs.findIndex((tab) => targetIdSet.has(tab.id));
  const activeTabClosed = targetIdSet.has(activeTabId.value);
  viewerCacheExclusions.value = targetTabs.map((tab) => viewerShellName(tab.id));
  await nextTick();
  if (spaceOpenTabId.value && targetIdSet.has(spaceOpenTabId.value)) {
    await cancelSpaceOpenLink();
  }
  await Promise.all(targetTabs.map((tab) => closeDocumentResources(tab.state)));
  for (let index = tabs.length - 1; index >= 0; index--) {
    if (targetIdSet.has(tabs[index].id)) tabs.splice(index, 1);
  }
  for (const tab of targetTabs) viewerShells.delete(tab.id);
  viewerCacheExclusions.value = [];
  if (activeTabClosed) {
    activeTabId.value = "";
    const nextTab = tabs[Math.min(firstTargetIndex, tabs.length - 1)];
    if (nextTab) activateTab(nextTab.id);
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
  if (!documentState.value.file || documentState.value.file.source !== "local") return;
  toggleFavorite(documentState.value.file.path, documentState.value.file.name);
}

async function clearWorkspace() {
  await Promise.all(tabs.map((tab) => closeDocumentResources(tab.state)));
  tabs.splice(0);
  activeTabId.value = "";
  Object.assign(emptyDocumentState, createDocumentState());
  persistTabs();
}

async function returnHome() {
  if (updateLocked.value) return;
  workspaceOperations++;
  try { await returnHomeImpl(); }
  finally { workspaceOperations--; }
}

async function returnHomeImpl() {
  if (!tabs.length) return;
  if (!(await requestDirtyDecision([...tabs]))) return;
  await cancelSpaceOpenLink();
  await clearWorkspace();
}

async function closeWindow() {
  if (updateLocked.value) return;
  if (!isTauri() || windowClosePending) return;
  windowClosePending = true;
  try {
    const keepTabs = keepTabsOnWindowClose();
    if (!keepTabs && !(await requestDirtyDecision([...tabs]))) return;
    await cancelSpaceOpenLink();
    if (!keepTabs) await clearWorkspace();
    await invoke("close_main_window");
  } catch (error) {
    recordDiagnostic("lifecycle", "window-close-failed");
    toast.add({
      title: "无法关闭窗口",
      description: errorMessage(error),
      color: "error",
      icon: "i-lucide-triangle-alert",
    });
  } finally {
    windowClosePending = false;
  }
}

function keepTabsOnWindowClose() {
  return appSettings.residencyMode === "fast" && appSettings.closeBehavior === "keep-tabs";
}

const canSwitchSpaceOpen = computed(
  () =>
    activeTabId.value === spaceOpenTabId.value &&
    spaceOpenPaths.value.length > 1 &&
    !guardOpen.value &&
    !settingsOpen.value &&
    !libraryOpen.value &&
    !openUrlModalOpen.value,
);

defineShortcuts(
  computed(() => ({
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
  })),
);

onMounted(async () => {
  tabResizeObserver = new ResizeObserver(updateTabScrollState);
  if (tabStrip.value) tabResizeObserver.observe(tabStrip.value);
  if (!isTauri()) return;
  try {
    await Promise.all([appSettings.load(), libraryState.load()]);
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
    onWindowCloseRequested: closeWindow,
    onDragStateChange: (active) => {
      dragActive.value = active;
    },
    onOpenFile: selectFile,
    onCloseTab: () => closeTab(),
    onNextTab: () => cycleTab(1),
    onPreviousTab: () => cycleTab(-1),
  });
  try {
    disposeUpdates = await updates.initialize();
    disposeUpdateFocus = await getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) void notifyUpdate();
    });
    await notifyUpdate();
  } catch { recordDiagnostic("updater", "initialize-failed"); }
});

watch(
  () => tabs.length,
  () =>
    void nextTick(() => {
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
  disposeUpdates?.();
  disposeUpdateFocus?.();
  tabResizeObserver?.disconnect();
  disposeLifecycle?.();
  void Promise.all(tabs.map((tab) => closeDocumentResources(tab.state)));
});
</script>

<template>
  <AppShell :immersive="tabs.length > 0" @request-close="closeWindow">
    <template #title-content>
      <div
        v-if="tabs.length"
        class="workspace-tab-viewport relative h-full max-w-full"
        data-tauri-drag-region
      >
        <div
          ref="tabStrip"
          class="workspace-tab-strip flex h-full items-center gap-1 overflow-x-auto"
          role="tablist"
          aria-label="已打开文件"
          @scroll="updateTabScrollState"
          @wheel="handleTabWheel"
        >
          <UContextMenu
            v-for="tab in tabs"
            :key="tab.id"
            :items="tabContextItems(tab)"
            :content="{ alignOffset: -4 }"
          >
            <div
              :data-tab-id="tab.id"
              class="workspace-tab group flex h-7 max-w-52 shrink-0 items-center rounded-md border px-1 transition-colors"
              :class="
                tab.id === activeTabId
                  ? 'border-primary/30 bg-primary/10 text-primary'
                  : 'border-transparent text-muted hover:bg-accented hover:text-highlighted'
              "
              @contextmenu="activateTab(tab.id)"
            >
              <UTooltip
                :text="tab.state.file?.path ?? tab.fallbackName"
                :delay-duration="2000"
                :content="{ side: 'bottom', align: 'center', sideOffset: 6 }"
                :ui="{ content: 'max-w-md break-all font-mono' }"
              >
                <button
                  class="flex h-full min-w-0 items-center gap-2 px-2 text-xs"
                  role="tab"
                  :aria-selected="tab.id === activeTabId"
                  :tabindex="tab.id === activeTabId ? 0 : -1"
                  @click="activateTab(tab.id)"
                >
                  <span
                    v-if="tab.state.dirty"
                    class="size-1.5 shrink-0 rounded-full bg-amber-500"
                  />
                  <span class="truncate">{{ tab.state.file?.name ?? tab.fallbackName }}</span>
                </button>
              </UTooltip>
              <UTooltip text="关闭标签页" :delay-duration="500"
                ><UButton
                  class="workspace-tab-close"
                  color="neutral"
                  variant="ghost"
                  size="xs"
                  icon="i-lucide-x"
                  aria-label="关闭标签页"
                  @click.stop="closeTab(tab.id)"
              /></UTooltip>
            </div>
          </UContextMenu>
        </div>
        <div
          v-if="canScrollTabsLeft"
          class="workspace-tab-fade workspace-tab-fade-left"
          aria-hidden="true"
        />
        <div
          v-if="canScrollTabsRight"
          class="workspace-tab-fade workspace-tab-fade-right"
          aria-hidden="true"
        />
      </div>
    </template>

    <template #actions>
      <UTooltip text="回到首页" :delay-duration="500"
        ><UButton
          class="titlebar-action"
          color="neutral"
          variant="ghost"
          size="xs"
          icon="i-lucide-house"
          aria-label="回到首页"
          :disabled="!tabs.length"
          @click="returnHome"
      /></UTooltip>
      <UTooltip text="打开文件" :delay-duration="500"
        ><UButton
          class="titlebar-action"
          color="neutral"
          variant="ghost"
          size="xs"
          icon="i-lucide-folder-open"
          aria-label="打开文件"
          @click="selectFile"
      /></UTooltip>
      <UTooltip text="打开 URL" :delay-duration="500"
        ><UButton
          class="titlebar-action"
          color="neutral"
          variant="ghost"
          size="xs"
          icon="i-lucide-link"
          aria-label="打开 URL"
          @click="openUrlModalOpen = true"
      /></UTooltip>
      <UTooltip
        v-if="documentState.file?.source === 'local'"
        :text="currentFavorite ? '取消收藏' : '收藏文件'"
        :delay-duration="500"
        ><UButton
          class="titlebar-action"
          :class="{ 'titlebar-favorite-active': currentFavorite }"
          color="neutral"
          variant="ghost"
          size="xs"
          icon="i-lucide-star"
          :aria-label="currentFavorite ? '取消收藏' : '收藏文件'"
          @click="toggleCurrentFavorite"
      /></UTooltip>
      <UTooltip text="文件库" :delay-duration="500"
        ><UButton
          class="titlebar-action"
          color="neutral"
          variant="ghost"
          size="xs"
          icon="i-lucide-library"
          aria-label="文件库"
          @click="libraryOpen = true"
      /></UTooltip>
      <UTooltip text="设置" :delay-duration="500"
        ><UButton
          class="titlebar-action"
          color="neutral"
          variant="ghost"
          size="xs"
          icon="i-lucide-settings"
          aria-label="设置"
          @click="settingsInitialSection = 'general'; settingsOpen = true"
      /></UTooltip>
    </template>

    <div v-if="tabs.length" class="flex size-full min-h-0 flex-col">
      <div v-if="documentState.error" class="flex min-h-0 flex-1 flex-col">
        <div class="grid min-h-0 flex-1 place-items-center p-6">
          <ErrorState
            :error="documentState.error"
            :can-open-hex="
              ['binary-content', 'unsupported-encoding'].includes(documentState.error.code) ||
              documentState.file?.kind === 'ebook'
            "
            @dismiss="closeTab()"
            @open-hex="openCurrentAsHex"
          />
        </div>
        <ViewerFooter :file-size="documentState.file?.size"
          ><template #status
            >{{ (documentState.file?.extension || "未知格式").toUpperCase() }} · 打开失败</template
          ></ViewerFooter
        >
      </div>
      <div
        v-else-if="documentState.file && documentState.status === 'ready' && hasViewer"
        class="flex min-h-0 flex-1 flex-col"
      >
        <KeepAlive :exclude="viewerCacheExclusions" :max="12">
          <component :is="activeViewerShell" :key="activeTabId">
            <ViewerHost
              :key="
                activeTab?.id === spaceOpenTabId ? `space-open:${activeViewerKey}` : activeTabId
              "
              class="min-h-0 flex-1"
              :file="documentState.file"
              :content="documentState.content"
              :read-only="documentState.readOnly"
              :large-text-session="documentState.largeTextSession"
              @update:content="updateContent"
              @open-hex="openCurrentAsHex"
            />
          </component>
        </KeepAlive>
      </div>
      <div v-else-if="documentState.status === 'loading'" class="flex min-h-0 flex-1 flex-col">
        <div class="grid flex-1 place-items-center text-center">
          <div>
            <UIcon name="i-lucide-loader-circle" class="mx-auto size-8 animate-spin text-primary" />
            <p class="mt-3 text-sm text-muted">正在加载文件…</p>
          </div>
        </div>
        <ViewerFooter :file-size="documentState.file?.size"
          ><template #status>正在加载</template></ViewerFooter
        >
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

    <UModal
      :open="guardOpen"
      :dismissible="false"
      title="保存修改？"
      @update:open="handleGuardOpenChange"
    >
      <template #body
        ><p class="text-sm leading-6 text-muted">
          本次操作涉及的标签页中，共有
          {{ guardedDirtyTabs.length }} 个包含未保存修改。保存后继续，或放弃这些修改。
        </p></template
      >
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <UButton color="neutral" variant="ghost" label="取消" :disabled="guardSaving" @click="finishGuard(false)" />
          <UButton color="error" variant="soft" label="放弃修改并继续" :disabled="guardSaving" @click="finishGuard(true)" />
          <UButton icon="i-lucide-save" label="保存全部" :loading="guardSaving" :disabled="guardSaving" @click="saveAllFromGuard" />
        </div>
      </template>
    </UModal>
    <FileLibraryModal
      v-model:open="libraryOpen"
      @open="openPathAsTab"
      @favorite="toggleFavorite"
      @remove-recent="removeRecent"
    />
    <OpenUrlModal v-model:open="openUrlModalOpen" @open-url="openRemoteAsTab" />
    <SettingsModal v-model:open="settingsOpen" :initial-section="settingsInitialSection" @request-update="requestUpdate" />
    <UModal :open="updateLocked && updates.status.phase === 'installing'" :dismissible="false" title="正在安装更新">
      <template #body><p class="text-sm text-muted">正在安装并重启 KView，请稍候…</p></template>
    </UModal>
  </AppShell>
</template>
