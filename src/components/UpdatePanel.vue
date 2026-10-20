<script setup lang="ts">
import { computed, ref } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { useUpdatesStore } from "../state/updates";

const emit = defineEmits<{ "request-update": [] }>();
const updates = useUpdatesStore();
const checking = ref(false);
const actionError = ref("");
const phaseLabel = computed(() => ({
  idle: "可手动检查新版本", checking: "正在检查更新…", "up-to-date": "当前已是最新版本",
  available: "发现新版本", downloading: "正在下载更新…", ready: "更新已下载，等待安装",
  installing: "正在安装更新…", error: "更新未完成",
})[updates.status.phase]);
const progress = computed(() => {
  const { downloadedBytes, totalBytes } = updates.status;
  const downloaded = (downloadedBytes / 1024 / 1024).toFixed(1);
  return totalBytes
    ? `${downloaded} / ${(totalBytes / 1024 / 1024).toFixed(1)} MiB`
    : `已下载 ${downloaded} MiB`;
});
const canUpdate = computed(() => !!updates.status.version && !updates.busy);

async function check() {
  if (checking.value || updates.busy) return;
  checking.value = true;
  actionError.value = "";
  try { await updates.check(); }
  catch (error) { actionError.value = error instanceof Error ? error.message : "检查更新失败，请重试。"; }
  finally { checking.value = false; }
}
</script>

<template>
  <div class="w-full space-y-3 rounded-xl border border-muted p-5 text-left">
    <p class="text-sm font-medium">{{ isTauri() ? phaseLabel : "更新功能仅在桌面版中可用" }}</p>
    <p v-if="updates.status.version" class="text-sm text-muted">新版本 {{ updates.status.version }}</p>
    <p v-if="updates.status.notes" class="max-h-32 overflow-y-auto whitespace-pre-wrap break-words text-xs text-muted">{{ updates.status.notes }}</p>
    <template v-if="updates.status.phase === 'downloading'">
      <progress v-if="updates.status.totalBytes" class="w-full" :value="updates.status.downloadedBytes" :max="updates.status.totalBytes" />
      <p class="text-xs text-muted" aria-live="polite">{{ progress }}</p>
    </template>
    <p v-if="actionError || updates.status.errorMessage" class="text-sm text-error" role="status">{{ actionError || updates.status.errorMessage }}</p>
    <div class="flex flex-wrap gap-2">
      <UButton label="检查更新" color="neutral" variant="soft" :loading="checking || updates.status.phase === 'checking'" :disabled="!isTauri() || updates.busy" @click="check" />
      <UButton v-if="canUpdate" :label="updates.status.phase === 'ready' || updates.status.errorCode === 'update-install' ? '安装并重启' : '下载并更新'" @click="emit('request-update')" />
    </div>
    <p class="text-xs leading-5 text-muted">正式版自动检查更新；下载安装由你确认，安装前会处理未保存修改。</p>
  </div>
</template>
