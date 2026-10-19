<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { CSSProperties } from "vue";
import DOMPurify from "dompurify";
import type { LivePhotoDocument, OpenedFile } from "../types/document";
import { recordDiagnostic } from "../services/diagnostics";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{
  file: OpenedFile;
  content: string;
  structuredContent?: LivePhotoDocument;
  readOnly: boolean;
}>();

const viewport = ref<HTMLElement>();
const imageViewport = ref<HTMLElement>();
const imageFrame = ref<HTMLElement>();
const zoom = ref(1);
const rotation = ref(0);
const width = ref(0);
const height = ref(0);
const availableWidth = ref(1);
const availableHeight = ref(1);
const failed = ref(false);
const isDragging = ref(false);
const safeSource = ref("");
const livePhoto = ref<LivePhotoDocument>();
const video = ref<HTMLVideoElement>();
const videoUrl = ref("");
const playing = ref(false);
const videoError = ref("");
let imageObjectUrl: string | undefined;
let resizeObserver: ResizeObserver | undefined;
let dragPointerId: number | undefined;
let lastPointerX = 0;
let lastPointerY = 0;
let zoomRequestId = 0;

const rotated = computed(() => rotation.value % 180 !== 0);
const fitScale = computed(() => {
  if (!width.value || !height.value) return 1;
  const naturalWidth = rotated.value ? height.value : width.value;
  const naturalHeight = rotated.value ? width.value : height.value;
  return Math.min(1, availableWidth.value / naturalWidth, availableHeight.value / naturalHeight);
});
const displayedWidth = computed(() => width.value * fitScale.value * zoom.value);
const displayedHeight = computed(() => height.value * fitScale.value * zoom.value);
const frameWidth = computed(() => (rotated.value ? displayedHeight.value : displayedWidth.value));
const frameHeight = computed(() => (rotated.value ? displayedWidth.value : displayedHeight.value));
const canPan = computed(
  () => frameWidth.value > availableWidth.value || frameHeight.value > availableHeight.value,
);
const contentStyle = computed(() => ({
  width: `${Math.max(availableWidth.value, frameWidth.value)}px`,
  height: `${Math.max(availableHeight.value, frameHeight.value)}px`,
}));
const frameStyle = computed(() => ({
  width: `${frameWidth.value}px`,
  height: `${frameHeight.value}px`,
  transform: "translate(-50%, -50%)",
}));
const imageStyle = computed<CSSProperties>(() => ({
  width: `${displayedWidth.value}px`,
  height: `${displayedHeight.value}px`,
  transform: `translate(-50%, -50%) rotate(${rotation.value}deg)`,
  visibility: width.value && height.value ? "visible" : "hidden",
}));
const livePhotoSourceLabel = computed(() => {
  if (!livePhoto.value) return "";
  return {
    livp: "LIVP",
    paired: "图片 + 视频",
    "motion-photo": "Motion Photo",
  }[livePhoto.value.sourceKind];
});

function clampZoom(value: number) {
  zoom.value = Math.min(8, Math.max(0.1, value));
}

function rotate() {
  rotation.value = (rotation.value + 90) % 360;
}

async function handleWheel(event: WheelEvent) {
  const element = imageViewport.value;
  const frame = imageFrame.value;
  if (!element || !frame) return;

  const frameBounds = frame.getBoundingClientRect();
  const focalX = frameBounds.width ? (event.clientX - frameBounds.left) / frameBounds.width : 0.5;
  const focalY = frameBounds.height ? (event.clientY - frameBounds.top) / frameBounds.height : 0.5;
  const requestId = ++zoomRequestId;

  clampZoom(zoom.value * Math.exp(-event.deltaY * 0.0015));
  await nextTick();
  if (requestId !== zoomRequestId) return;

  const nextFrameBounds = frame.getBoundingClientRect();
  const focalClientX = nextFrameBounds.left + nextFrameBounds.width * focalX;
  const focalClientY = nextFrameBounds.top + nextFrameBounds.height * focalY;
  element.scrollLeft += focalClientX - event.clientX;
  element.scrollTop += focalClientY - event.clientY;
}

function startDragging(event: PointerEvent) {
  const element = imageViewport.value;
  if (!element || !canPan.value || event.button !== 0) return;

  event.preventDefault();
  dragPointerId = event.pointerId;
  lastPointerX = event.clientX;
  lastPointerY = event.clientY;
  isDragging.value = true;
  element.setPointerCapture(event.pointerId);
}

async function startLivePhoto(event?: Event) {
  if (!livePhoto.value || !video.value || videoError.value) return;
  event?.preventDefault();
  playing.value = true;
  video.value.currentTime = 0;
  try {
    await video.value.play();
  } catch {
    playing.value = false;
    videoError.value = "动态部分不受系统解码器支持";
    recordDiagnostic("viewer", "live-photo-video-decode-failed");
  }
}

function stopLivePhoto(event?: Event) {
  if (!livePhoto.value || !video.value) return;
  event?.preventDefault();
  video.value.pause();
  video.value.currentTime = 0;
  playing.value = false;
}

function handleLivePhotoKeyDown(event: KeyboardEvent) {
  if ((event.key === " " || event.key === "Enter") && !event.repeat) {
    void startLivePhoto(event);
  }
}

function handleLivePhotoKeyUp(event: KeyboardEvent) {
  if (event.key === " " || event.key === "Enter") stopLivePhoto(event);
}

function dragImage(event: PointerEvent) {
  const element = imageViewport.value;
  if (!element || !isDragging.value || event.pointerId !== dragPointerId) return;

  element.scrollLeft -= event.clientX - lastPointerX;
  element.scrollTop -= event.clientY - lastPointerY;
  lastPointerX = event.clientX;
  lastPointerY = event.clientY;
}

function stopDragging(event: PointerEvent) {
  const element = imageViewport.value;
  if (!element || event.pointerId !== dragPointerId) return;

  if (element.hasPointerCapture(event.pointerId)) {
    element.releasePointerCapture(event.pointerId);
  }
  dragPointerId = undefined;
  isDragging.value = false;
}

async function toggleFullscreen() {
  if (!document.fullscreenElement) await viewport.value?.requestFullscreen();
  else await document.exitFullscreen();
}

function onImageLoad(event: Event) {
  failed.value = false;
  const image = event.currentTarget as HTMLImageElement;
  width.value = image.naturalWidth;
  height.value = image.naturalHeight;
}

function onImageError() {
  failed.value = true;
  recordDiagnostic("viewer", "image-decode-failed");
}

function onVideoError() {
  playing.value = false;
  videoError.value = "动态部分不受系统解码器支持";
  recordDiagnostic("viewer", "live-photo-video-decode-failed");
}

function revokeMediaUrls() {
  if (imageObjectUrl) {
    URL.revokeObjectURL(imageObjectUrl);
    imageObjectUrl = undefined;
  }
  if (videoUrl.value) {
    videoUrl.value = "";
  }
}

function prepareMedia() {
  revokeMediaUrls();
  livePhoto.value = undefined;
  playing.value = false;
  videoError.value = "";
  failed.value = false;
  width.value = 0;
  height.value = 0;

  if (props.file.kind === "live-photo") {
    try {
      const document =
        props.structuredContent ??
        (props.content ? (JSON.parse(props.content) as LivePhotoDocument) : null);
      if (!document) throw new Error("动态照片数据无效");
      livePhoto.value = document;
      safeSource.value = document.imageUrl;
      const probe = window.document.createElement("video");
      if (!probe.canPlayType(document.videoMimeType)) {
        videoError.value = "动态部分不受系统解码器支持";
        return;
      }
      videoUrl.value = document.videoUrl;
    } catch {
      failed.value = true;
      recordDiagnostic("viewer", "live-photo-parse-failed");
    }
    return;
  }

  safeSource.value = props.content;
  if (props.file.extension === "svg") {
    const encoded = props.content.split(",", 2)[1] ?? "";
    const source = new TextDecoder().decode(
      Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0)),
    );
    const sanitized = DOMPurify.sanitize(source, {
      USE_PROFILES: { svg: true, svgFilters: true },
      FORBID_TAGS: ["script", "foreignObject"],
    });
    imageObjectUrl = URL.createObjectURL(new Blob([sanitized], { type: "image/svg+xml" }));
    safeSource.value = imageObjectUrl;
  }
}

function updateAvailableSize() {
  const element = imageViewport.value;
  if (!element) return;
  const styles = window.getComputedStyle(element);
  const horizontalInsets =
    (Number.parseFloat(styles.paddingLeft) || 0) +
    (Number.parseFloat(styles.paddingRight) || 0) +
    (Number.parseFloat(styles.borderLeftWidth) || 0) +
    (Number.parseFloat(styles.borderRightWidth) || 0);
  const verticalInsets =
    (Number.parseFloat(styles.paddingTop) || 0) +
    (Number.parseFloat(styles.paddingBottom) || 0) +
    (Number.parseFloat(styles.borderTopWidth) || 0) +
    (Number.parseFloat(styles.borderBottomWidth) || 0);
  const bounds = element.getBoundingClientRect();

  // clientWidth/clientHeight 会随滚动条出现或消失而改变，临界缩放比例下会导致适配尺寸反复震荡。
  availableWidth.value = Math.max(1, bounds.width - horizontalInsets);
  availableHeight.value = Math.max(1, bounds.height - verticalInsets);
}

onMounted(() => {
  resizeObserver = new ResizeObserver(updateAvailableSize);
  if (imageViewport.value) resizeObserver.observe(imageViewport.value);
  updateAvailableSize();
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  revokeMediaUrls();
});

watch(() => [props.file.path, props.file.modifiedAt, props.content] as const, prepareMedia, {
  immediate: true,
});
</script>

<template>
  <div ref="viewport" class="relative flex size-full min-h-0 flex-col bg-muted">
    <div
      ref="imageViewport"
      class="min-h-0 flex-1 overflow-auto p-8"
      :class="canPan ? (isDragging ? 'cursor-grabbing' : 'cursor-grab') : ''"
      :tabindex="livePhoto ? 0 : undefined"
      :aria-label="livePhoto ? '按住播放动态照片' : undefined"
      @pointerdown="startDragging"
      @pointerdown.left="startLivePhoto"
      @pointermove="dragImage"
      @pointerup="
        stopDragging($event);
        stopLivePhoto($event);
      "
      @pointercancel="
        stopDragging($event);
        stopLivePhoto($event);
      "
      @pointerleave="stopLivePhoto"
      @keydown="handleLivePhotoKeyDown"
      @keyup="handleLivePhotoKeyUp"
      @wheel.prevent="handleWheel"
    >
      <div v-if="failed" class="grid size-full place-items-center">
        <UAlert
          class="max-w-md"
          color="error"
          variant="subtle"
          icon="i-lucide-image-off"
          title="图片无法显示"
          description="文件可能已损坏，或使用了当前系统 WebView 不支持的编码。"
        />
      </div>
      <div v-else class="relative" :style="contentStyle">
        <div ref="imageFrame" class="absolute left-1/2 top-1/2" :style="frameStyle">
          <img
            :src="safeSource"
            :alt="file.name"
            class="absolute left-1/2 top-1/2 max-w-none select-none object-contain shadow-lg"
            :style="imageStyle"
            draggable="false"
            @load="onImageLoad"
            @error="onImageError"
          />
          <video
            v-if="livePhoto && videoUrl"
            ref="video"
            :src="videoUrl"
            class="absolute left-1/2 top-1/2 max-w-none select-none object-contain shadow-lg transition-opacity"
            :class="playing ? 'opacity-100' : 'pointer-events-none opacity-0'"
            :style="imageStyle"
            playsinline
            preload="auto"
            @ended="stopLivePhoto"
            @error="onVideoError"
          />
          <span
            v-if="livePhoto"
            class="pointer-events-none absolute left-2 top-2 rounded-md bg-black/65 px-2 py-1 text-[11px] font-semibold tracking-wide text-white backdrop-blur"
          >
            LIVE
          </span>
        </div>
      </div>
    </div>
    <ViewerFooter :file-size="file.size">
      <template #status>
        {{ file.extension.toUpperCase() }}
        <template v-if="livePhotoSourceLabel"> · {{ livePhotoSourceLabel }}</template>
        <template v-if="width"> · {{ width }} × {{ height }}</template>
        <template v-if="videoError"> · {{ videoError }}</template>
      </template>
      <template #actions>
        <UTooltip text="缩小" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            color="neutral"
            variant="ghost"
            size="xs"
            icon="i-lucide-zoom-out"
            aria-label="缩小"
            @click="clampZoom(zoom - 0.1)"
        /></UTooltip>
        <span class="w-12 text-center text-xs text-muted">{{ Math.round(zoom * 100) }}%</span>
        <UTooltip text="放大" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            color="neutral"
            variant="ghost"
            size="xs"
            icon="i-lucide-zoom-in"
            aria-label="放大"
            @click="clampZoom(zoom + 0.1)"
        /></UTooltip>
        <UTooltip text="顺时针旋转" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            color="neutral"
            variant="ghost"
            size="xs"
            icon="i-lucide-rotate-cw"
            aria-label="顺时针旋转"
            @click="rotate"
        /></UTooltip>
        <UTooltip text="全屏" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            color="neutral"
            variant="ghost"
            size="xs"
            icon="i-lucide-maximize"
            aria-label="全屏"
            @click="toggleFullscreen"
        /></UTooltip>
      </template>
    </ViewerFooter>
  </div>
</template>
