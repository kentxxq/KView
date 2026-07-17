<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { OpenedFile } from "../types/document";
import { decodeBase64 } from "../utils/binary";
import ViewerFooter from "../components/ViewerFooter.vue";

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
    <div v-if="error" class="grid flex-1 place-items-center p-8 text-center">
      <div>
        <UIcon name="i-lucide-circle-slash-2" class="mx-auto mb-3 size-9 text-red-400" />
        <p class="text-sm text-red-300">{{ error }}</p>
      </div>
    </div>
    <div v-else class="grid min-h-0 flex-1 place-items-center overflow-hidden bg-black p-4">
      <video
        ref="video"
        class="block max-h-full max-w-full object-contain bg-black"
        :src="url"
        controls
        playsinline
        @loadedmetadata="duration = video?.duration ?? 0; dimensions = `${video?.videoWidth ?? 0} × ${video?.videoHeight ?? 0}`"
        @error="error = '视频无法播放：编码可能不受当前系统 WebView 支持，或文件已经损坏。'"
      />
    </div>
    <ViewerFooter>
      <template #status>
        {{ file.extension.toUpperCase() }}
        <template v-if="dimensions"> · {{ dimensions }}</template>
        <template v-if="duration"> · {{ formatDuration(duration) }}</template>
        · {{ error ? "播放失败" : "只读" }}
      </template>
    </ViewerFooter>
  </div>
</template>
