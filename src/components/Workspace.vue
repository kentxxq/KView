<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import AppShell from "./AppShell.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import FileReadyState from "./FileReadyState.vue";
import SettingsModal from "./SettingsModal.vue";
import ViewerHost from "../viewers/ViewerHost.vue";
import { resolveViewer } from "../viewers/registry";
import { documentState, resetDocument } from "../state/document";
import type {
  ImageDocument,
  OpenedFile,
  SaveResult,
  TextDocument,
} from "../types/document";
import { initializeDesktopLifecycle } from "../services/desktop-lifecycle";
import { recordDiagnostic } from "../services/diagnostics";
import { loadSettings } from "../state/settings";

const toast = useToast();
const dragActive = ref(false);
const guardOpen = ref(false);
const settingsOpen = ref(false);
let disposeLifecycle: (() => void) | undefined;
let resolveGuard: ((result: boolean) => void) | undefined;

const hasViewer = computed(
  () => !!documentState.file && !!resolveViewer(documentState.file),
);
const canSave = computed(
  () =>
    !!documentState.file &&
    !documentState.readOnly &&
    documentState.viewerKind !== "image" &&
    documentState.status === "ready",
);

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

async function requestDirtyDecision() {
  if (!documentState.dirty) return true;
  if (resolveGuard) return false;

  guardOpen.value = true;
  return new Promise<boolean>((resolve) => {
    resolveGuard = resolve;
  });
}

function finishGuard(result: boolean) {
  guardOpen.value = false;
  resolveGuard?.(result);
  resolveGuard = undefined;
}

function handleGuardOpenChange(open: boolean) {
  if (!open && resolveGuard) finishGuard(false);
}

async function saveFromGuard() {
  if (await saveDocument(false)) finishGuard(true);
}

async function loadFile(path: string) {
  const file = await invoke<OpenedFile>("inspect_file", { path });
  const definition = resolveViewer(file);
  if (!definition) {
    throw new Error(`暂不支持 .${file.extension || "未知"} 文件`);
  }

  documentState.file = file;
  documentState.viewerKind = file.kind;
  documentState.sourceModifiedAt = file.modifiedAt;
  documentState.dirty = false;
  documentState.truncated = false;

  if (file.kind === "image") {
    const image = await invoke<ImageDocument>("read_image", { path: file.path });
    documentState.buffer = image.dataUrl;
    documentState.readOnly = true;
  } else {
    const text = await invoke<TextDocument>("read_text", { path: file.path });
    documentState.buffer = text.content;
    documentState.encoding = text.encoding;
    documentState.lineEnding = text.lineEnding;
    documentState.bom = text.bom;
    documentState.readOnly = text.readOnly;
    documentState.truncated = text.truncated;
    file.encoding = text.encoding;
    file.lineEnding = text.lineEnding;
    file.readOnly = text.readOnly;
  }
  documentState.status = "ready";
}

async function openPaths(paths: string[]) {
  const availablePaths = paths.filter(Boolean);
  if (!availablePaths.length) return;
  if (!(await requestDirtyDecision())) return;

  if (availablePaths.length > 1) {
    toast.add({
      title: "暂不支持批量打开",
      description: `已打开第一个文件，其余 ${availablePaths.length - 1} 个文件已忽略。`,
      color: "warning",
      icon: "i-lucide-info",
      duration: 8000,
    });
  }

  resetDocument();
  documentState.status = "loading";
  const startedAt = performance.now();

  try {
    await loadFile(availablePaths[0]);
    recordDiagnostic(
      "performance",
      `open-${documentState.viewerKind}`,
      performance.now() - startedAt,
    );
  } catch (error) {
    recordDiagnostic("viewer", "open-failed", performance.now() - startedAt);
    documentState.status = "error";
    documentState.error = {
      code: "open-failed",
      title: "无法打开文件",
      message: errorMessage(error),
      recoverable: true,
    };
  }
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
}

async function saveDocument(saveAs: boolean) {
  if (!canSave.value || !documentState.file || documentState.buffer === null) return false;

  let targetPath = documentState.file.path;
  if (saveAs) {
    const selected = await save({
      defaultPath: documentState.file.path,
      filters: [{ name: "文本文件", extensions: [documentState.file.extension || "txt"] }],
    });
    if (!selected) return false;
    targetPath = selected;
  }

  try {
    const result = await invoke<SaveResult>("save_text", {
      path: targetPath,
      content: documentState.buffer,
      encoding: documentState.encoding ?? "utf-8",
      lineEnding: documentState.lineEnding ?? "lf",
      bom: documentState.bom,
      expectedModifiedAt: saveAs ? null : documentState.sourceModifiedAt,
    });
    const refreshed = await invoke<OpenedFile>("inspect_file", { path: result.path });
    refreshed.encoding = documentState.encoding;
    refreshed.lineEnding = documentState.lineEnding;
    refreshed.readOnly = false;
    documentState.file = refreshed;
    documentState.sourceModifiedAt = result.modifiedAt;
    documentState.dirty = false;
    toast.add({
      title: saveAs ? "文件已另存" : "文件已保存",
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

defineShortcuts({
  meta_s: () => saveDocument(false),
  meta_shift_s: () => saveDocument(true),
});

onMounted(async () => {
  if (!isTauri()) return;
  try {
    await loadSettings();
  } catch {
    recordDiagnostic("settings", "load-failed");
  }
  disposeLifecycle = await initializeDesktopLifecycle({
    onPaths: openPaths,
    onRequestClose: requestDirtyDecision,
    onDragStateChange: (active) => {
      dragActive.value = active;
    },
  });
});

onBeforeUnmount(() => disposeLifecycle?.());
</script>

<template>
  <AppShell :immersive="hasViewer && documentState.status === 'ready'">
    <template #actions>
      <div class="flex items-center gap-1">
        <template v-if="canSave">
          <UBadge v-if="documentState.dirty" color="warning" variant="subtle">未保存</UBadge>
          <UButton
            color="neutral"
            variant="ghost"
            icon="i-lucide-save"
            label="保存"
            :disabled="!documentState.dirty"
            @click="saveDocument(false)"
          />
          <UButton color="neutral" variant="ghost" label="另存为" @click="saveDocument(true)" />
        </template>
        <UButton
          v-if="documentState.file"
          color="neutral"
          variant="ghost"
          icon="i-lucide-folder-open"
          label="打开"
          @click="selectFile"
        />
        <UBadge v-else color="neutral" variant="subtle" size="sm">本地模式</UBadge>
        <UButton
          color="neutral"
          variant="ghost"
          icon="i-lucide-settings"
          aria-label="设置"
          @click="settingsOpen = true"
        />
      </div>
    </template>

    <template #status>
      <span v-if="documentState.file" class="max-w-96 truncate">{{ documentState.file.path }}</span>
      <span v-else>准备就绪</span>
      <span v-if="documentState.file && documentState.encoding">
        {{ documentState.encoding.toUpperCase() }} · {{ documentState.lineEnding?.toUpperCase() }}
      </span>
      <span v-else>文件仅在本机处理</span>
    </template>

    <ErrorState
      v-if="documentState.error"
      :error="documentState.error"
      @dismiss="resetDocument"
    />
    <div
      v-else-if="documentState.file && documentState.status === 'ready' && hasViewer"
      class="flex size-full min-h-0 flex-col"
    >
      <UAlert
        v-if="documentState.truncated"
        class="m-3 shrink-0"
        color="warning"
        variant="subtle"
        icon="i-lucide-file-warning"
        title="大文件只读预览"
        description="文件超过 20 MiB，仅加载前 2 MiB 内容以保持界面流畅。"
      />
      <ViewerHost
        class="min-h-0 flex-1"
        :file="documentState.file"
        :content="documentState.buffer ?? ''"
        :read-only="documentState.readOnly"
        @update:content="updateContent"
      />
    </div>
    <FileReadyState
      v-else-if="documentState.file && documentState.status === 'ready'"
      :file="documentState.file"
      @open="selectFile"
    />
    <div v-else-if="documentState.status === 'loading'" class="text-center">
      <UIcon name="i-lucide-loader-circle" class="mx-auto size-8 animate-spin text-primary" />
      <p class="mt-3 text-sm text-muted">正在加载文件…</p>
    </div>
    <EmptyState v-else :drag-active="dragActive" @select="selectFile" />

    <UModal
      :open="guardOpen"
      :dismissible="false"
      title="保存修改？"
      @update:open="handleGuardOpenChange"
    >
      <template #body>
        <p class="text-sm leading-6 text-muted">
          {{ documentState.file?.name }} 包含未保存的修改。保存后继续，或放弃本次修改。
        </p>
      </template>
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <UButton color="neutral" variant="ghost" label="取消" @click="finishGuard(false)" />
          <UButton color="error" variant="soft" label="放弃修改" @click="finishGuard(true)" />
          <UButton icon="i-lucide-save" label="保存" @click="saveFromGuard" />
        </div>
      </template>
    </UModal>
    <SettingsModal v-model:open="settingsOpen" />
  </AppShell>
</template>
