<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { OpenedFile } from "../types/document";
import { decodeBase64 } from "../utils/binary";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const video = ref<HTMLVideoElement>();
const url = ref("");
const error = ref("");
const duration = ref(0);
const dimensions = ref("");

const mimeType = computed(() => ({
  mp4: "video/mp4",
  webm: "video/webm",
  mov: "video/quicktime",
}[props.file.extension] ?? "application/octet-stream"));

function formatDuration(seconds: number) {
  if (!Number.isFinite(seconds)) return "未知时长";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`;
}

onMounted(() => {
  const probe = document.createElement("video");
  if (!probe.canPlayType(mimeType.value)) {
    error.value = `${props.file.extension.toUpperCase()} 的编码或容器不受当前系统 WebView 支持。`;
    return;
  }
  const bytes = decodeBase64(props.content);
  url.value = URL.createObjectURL(new Blob([bytes], { type: mimeType.value }));
});

onBeforeUnmount(() => {
  if (url.value) URL.revokeObjectURL(url.value);
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-black text-white">
    <div class="flex h-11 shrink-0 items-center gap-3 border-b border-white/10 px-4 text-xs text-white/70">
      <span>{{ file.extension.toUpperCase() }}</span>
      <span v-if="dimensions">{{ dimensions }}</span>
      <span v-if="duration">{{ formatDuration(duration) }}</span>
      <span class="ml-auto">由系统 WebView 解码 · 只读</span>
    </div>
    <div v-if="error" class="grid flex-1 place-items-center p-8 text-center">
      <div>
        <UIcon name="i-lucide-circle-slash-2" class="mx-auto mb-3 size-9 text-red-400" />
        <p class="text-sm text-red-300">{{ error }}</p>
      </div>
    </div>
    <div v-else class="grid min-h-0 flex-1 place-items-center p-4">
      <video
        ref="video"
        class="max-h-full max-w-full rounded-lg bg-black shadow-2xl"
        :src="url"
        controls
        playsinline
        @loadedmetadata="duration = video?.duration ?? 0; dimensions = `${video?.videoWidth ?? 0} × ${video?.videoHeight ?? 0}`"
        @error="error = '视频无法播放：编码可能不受当前系统 WebView 支持，或文件已经损坏。'"
      />
    </div>
  </div>
</template>

