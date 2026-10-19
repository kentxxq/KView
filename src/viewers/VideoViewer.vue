<script setup lang="ts">
import Artplayer from "artplayer";
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import type { OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{
  file: OpenedFile;
  content: string;
  binaryContent?: Uint8Array<ArrayBuffer>;
  readOnly: boolean;
}>();
const container = ref<HTMLDivElement>();
const player = shallowRef<Artplayer>();
const url = ref("");
const error = ref("");
const duration = ref(0);
const dimensions = ref("");

const mimeType = computed(
  () =>
    ({
      mp4: "video/mp4",
      webm: "video/webm",
      mov: "video/quicktime",
    })[props.file.extension] ?? "application/octet-stream",
);

function formatDuration(seconds: number) {
  if (!Number.isFinite(seconds)) return "未知时长";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${Math.floor(seconds % 60)
    .toString()
    .padStart(2, "0")}`;
}

function updateMetadata() {
  if (!player.value) return;
  const video = player.value.video;
  duration.value = video.duration;
  dimensions.value = `${video.videoWidth} × ${video.videoHeight}`;
}

function handlePlaybackError() {
  error.value = "视频无法播放：编码可能不受当前系统 WebView 支持，或文件已经损坏。";
  player.value?.pause();
}

let localUrl = "";

function loadVideo() {
  player.value?.destroy(false);
  player.value = undefined;
  if (localUrl) {
    URL.revokeObjectURL(localUrl);
    localUrl = "";
  }
  url.value = "";
  error.value = "";
  duration.value = 0;
  dimensions.value = "";
  const probe = document.createElement("video");
  if (!probe.canPlayType(mimeType.value)) {
    error.value = `${props.file.extension.toUpperCase()} 的编码或容器不受当前系统 WebView 支持。`;
    return;
  }

  if (props.content) {
    url.value = props.content;
  } else if (props.binaryContent) {
    localUrl = URL.createObjectURL(new Blob([props.binaryContent], { type: mimeType.value }));
    url.value = localUrl;
  } else {
    error.value = "视频内容不可用。";
    return;
  }
  if (!container.value) return;

  const instance = new Artplayer({
    container: container.value,
    url: url.value,
    type: props.file.extension,
    theme: "#0ea5e9",
    lang: "zh-cn",
    autoplay: false,
    loop: false,
    setting: false,
    hotkey: false,
    pip: false,
    backdrop: false,
    fullscreen: true,
    fullscreenWeb: false,
    screenshot: false,
    flip: false,
    playbackRate: false,
    aspectRatio: false,
    subtitleOffset: false,
    miniProgressBar: false,
    moreVideoAttr: {
      preload: "metadata",
      playsInline: true,
    },
  });
  player.value = instance;
  instance.on("video:loadedmetadata", updateMetadata);
  instance.on("video:error", handlePlaybackError);
  if (instance.video.readyState >= HTMLMediaElement.HAVE_METADATA) updateMetadata();
}

onMounted(loadVideo);
watch(() => [props.file.path, props.file.modifiedAt, props.content, props.binaryContent], loadVideo);

onBeforeUnmount(() => {
  player.value?.destroy(false);
  player.value = undefined;
  if (localUrl) {
    URL.revokeObjectURL(localUrl);
    localUrl = "";
  }
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-black text-white">
    <div class="relative min-h-0 flex-1 overflow-hidden bg-black p-4">
      <div ref="container" class="size-full overflow-hidden bg-black" />
      <div v-if="error" class="absolute inset-0 grid place-items-center bg-black p-8 text-center">
        <div>
          <UIcon name="i-lucide-circle-slash-2" class="mx-auto mb-3 size-9 text-red-400" />
          <p class="text-sm text-red-300">{{ error }}</p>
        </div>
      </div>
    </div>
    <ViewerFooter :file-size="file.size">
      <template #status>
        {{ file.extension.toUpperCase() }}
        <template v-if="dimensions"> · {{ dimensions }}</template>
        <template v-if="duration"> · {{ formatDuration(duration) }}</template>
        · {{ error ? "播放失败" : "只读" }}
      </template>
    </ViewerFooter>
  </div>
</template>
