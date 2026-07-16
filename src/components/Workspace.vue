<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import AppShell from "./AppShell.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import FileReadyState from "./FileReadyState.vue";
import { documentState, resetDocument } from "../state/document";
import type { OpenedFile } from "../types/document";
import { initializeDesktopLifecycle } from "../services/desktop-lifecycle";

const toast = useToast();
const dragActive = ref(false);
let disposeLifecycle: (() => void) | undefined;

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

async function openPaths(paths: string[]) {
  const availablePaths = paths.filter(Boolean);
  if (!availablePaths.length) return;

  if (availablePaths.length > 1) {
    toast.add({
      title: "暂不支持批量打开",
      description: `已打开第一个文件，其余 ${availablePaths.length - 1} 个文件已忽略。`,
      color: "warning",
      icon: "i-lucide-info",
      duration: 8000,
    });
  }

  documentState.status = "loading";
  documentState.error = null;

  try {
    const file = await invoke<OpenedFile>("inspect_file", { path: availablePaths[0] });
    documentState.file = file;
    documentState.viewerKind = file.kind;
    documentState.sourceModifiedAt = file.modifiedAt;
    documentState.status = "ready";
  } catch (error) {
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
  if (selection) {
    await openPaths(Array.isArray(selection) ? selection : [selection]);
  }
}

onMounted(async () => {
  if (!isTauri()) return;

  disposeLifecycle = await initializeDesktopLifecycle({
    onPaths: openPaths,
    onDragStateChange: (active) => {
      dragActive.value = active;
    },
  });
});

onBeforeUnmount(() => disposeLifecycle?.());
</script>

<template>
  <AppShell>
    <template #actions>
      <UButton
        v-if="documentState.file"
        color="neutral"
        variant="ghost"
        icon="i-lucide-folder-open"
        label="打开文件"
        @click="selectFile"
      />
      <UBadge v-else color="neutral" variant="subtle" size="sm">本地模式</UBadge>
    </template>

    <ErrorState
      v-if="documentState.error"
      :error="documentState.error"
      @dismiss="resetDocument"
    />
    <FileReadyState
      v-else-if="documentState.file && documentState.status === 'ready'"
      :file="documentState.file"
      @open="selectFile"
    />
    <div v-else-if="documentState.status === 'loading'" class="text-center">
      <UIcon name="i-lucide-loader-circle" class="mx-auto size-8 animate-spin text-primary" />
      <p class="mt-3 text-sm text-muted">正在读取文件信息…</p>
    </div>
    <EmptyState v-else :drag-active="dragActive" @select="selectFile" />
  </AppShell>
</template>
