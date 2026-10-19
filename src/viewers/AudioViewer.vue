<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{
  file: OpenedFile;
  content: string;
  binaryContent?: Uint8Array<ArrayBuffer>;
  readOnly: boolean;
}>();
const audio = ref<HTMLAudioElement>();
const url = ref("");
const error = ref("");
const duration = ref(0);

const mimeType = computed(
  () =>
    ({
      mp3: "audio/mpeg",
      wav: "audio/wav",
      m4a: "audio/mp4",
      aac: "audio/aac",
      flac: "audio/flac",
      ogg: "audio/ogg",
      opus: "audio/ogg; codecs=opus",
    })[props.file.extension] ?? "application/octet-stream",
);

function formatDuration(seconds: number) {
  if (!Number.isFinite(seconds)) return "未知时长";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = Math.floor(seconds % 60)
    .toString()
    .padStart(2, "0");
  return hours
    ? `${hours}:${minutes.toString().padStart(2, "0")}:${remainingSeconds}`
    : `${minutes}:${remainingSeconds}`;
}

let localUrl = "";

function loadAudio() {
  audio.value?.pause();
  if (localUrl) {
    URL.revokeObjectURL(localUrl);
    localUrl = "";
  }
  url.value = "";
  error.value = "";
  duration.value = 0;
  const probe = document.createElement("audio");
  if (!probe.canPlayType(mimeType.value)) {
    error.value = `${props.file.extension.toUpperCase()} 不受当前系统 WebView 支持。`;
    return;
  }
  if (props.content) {
    url.value = props.content;
  } else if (props.binaryContent) {
    localUrl = URL.createObjectURL(new Blob([props.binaryContent], { type: mimeType.value }));
    url.value = localUrl;
  } else {
    error.value = "音频内容不可用";
    return;
  }
}

onMounted(loadAudio);
watch(() => [props.file.path, props.file.modifiedAt, props.content, props.binaryContent], loadAudio);

onBeforeUnmount(() => {
  audio.value?.pause();
  if (localUrl) {
    URL.revokeObjectURL(localUrl);
    localUrl = "";
  }
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-default">
    <div class="grid min-h-0 flex-1 place-items-center overflow-auto p-8">
      <div class="w-full max-w-xl rounded-2xl border border-muted bg-elevated p-8 shadow-sm">
        <div class="mb-8 flex flex-col items-center text-center">
          <div class="mb-4 grid size-20 place-items-center rounded-2xl bg-primary/10 text-primary">
            <UIcon name="i-lucide-audio-lines" class="size-10" />
          </div>
          <p class="max-w-full truncate text-base font-medium text-highlighted">{{ file.name }}</p>
          <p class="mt-1 text-xs text-muted">{{ file.extension.toUpperCase() }}</p>
        </div>

        <div v-if="error" class="text-center">
          <UIcon name="i-lucide-circle-slash-2" class="mx-auto mb-3 size-8 text-error" />
          <p class="text-sm text-error">{{ error }}</p>
        </div>
        <audio
          v-else
          ref="audio"
          class="block w-full"
          :src="url"
          controls
          preload="metadata"
          @loadedmetadata="duration = audio?.duration ?? 0"
          @error="error = '音频无法播放：编码可能不受当前系统 WebView 支持，或文件已经损坏。'"
        />
      </div>
    </div>
    <ViewerFooter :file-size="file.size">
      <template #status>
        {{ file.extension.toUpperCase() }}
        <template v-if="duration"> · {{ formatDuration(duration) }}</template>
        · {{ error ? "播放失败" : "只读" }}
      </template>
    </ViewerFooter>
  </div>
</template>
