<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { CSSProperties } from "vue";
import DOMPurify from "dompurify";
import type { OpenedFile } from "../types/document";
import { recordDiagnostic } from "../services/diagnostics";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();

const viewport = ref<HTMLElement>();
const imageViewport = ref<HTMLElement>();
const zoom = ref(1);
const rotation = ref(0);
const width = ref(0);
const height = ref(0);
const availableWidth = ref(1);
const availableHeight = ref(1);
const failed = ref(false);
const safeSource = ref(props.content);
let objectUrl: string | undefined;
let resizeObserver: ResizeObserver | undefined;

const rotated = computed(() => rotation.value % 180 !== 0);
const fitScale = computed(() => {
  if (!width.value || !height.value) return 1;
  const naturalWidth = rotated.value ? height.value : width.value;
  const naturalHeight = rotated.value ? width.value : height.value;
  return Math.min(
    1,
    availableWidth.value / naturalWidth,
    availableHeight.value / naturalHeight,
  );
});
const displayedWidth = computed(() => width.value * fitScale.value * zoom.value);
const displayedHeight = computed(() => height.value * fitScale.value * zoom.value);
const frameWidth = computed(() => rotated.value ? displayedHeight.value : displayedWidth.value);
const frameHeight = computed(() => rotated.value ? displayedWidth.value : displayedHeight.value);
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

function clampZoom(value: number) {
  zoom.value = Math.min(8, Math.max(0.1, value));
}

function rotate() {
  rotation.value = (rotation.value + 90) % 360;
}

function handleWheel(event: WheelEvent) {
  clampZoom(zoom.value * Math.exp(-event.deltaY * 0.0015));
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

function updateAvailableSize() {
  const element = imageViewport.value;
  if (!element) return;
  const styles = window.getComputedStyle(element);
  const horizontalPadding = (Number.parseFloat(styles.paddingLeft) || 0) + (Number.parseFloat(styles.paddingRight) || 0);
  const verticalPadding = (Number.parseFloat(styles.paddingTop) || 0) + (Number.parseFloat(styles.paddingBottom) || 0);
  availableWidth.value = Math.max(1, element.clientWidth - horizontalPadding);
  availableHeight.value = Math.max(1, element.clientHeight - verticalPadding);
}

onMounted(() => {
  if (props.file.extension === "svg") {
    const encoded = props.content.split(",", 2)[1] ?? "";
    const source = new TextDecoder().decode(
      Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0)),
    );
    const sanitized = DOMPurify.sanitize(source, {
      USE_PROFILES: { svg: true, svgFilters: true },
      FORBID_TAGS: ["script", "foreignObject"],
    });
    objectUrl = URL.createObjectURL(new Blob([sanitized], { type: "image/svg+xml" }));
    safeSource.value = objectUrl;
  }
  resizeObserver = new ResizeObserver(updateAvailableSize);
  if (imageViewport.value) resizeObserver.observe(imageViewport.value);
  updateAvailableSize();
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  if (objectUrl) URL.revokeObjectURL(objectUrl);
});
</script>

<template>
  <div ref="viewport" class="relative flex size-full min-h-0 flex-col bg-muted">
    <div ref="imageViewport" class="min-h-0 flex-1 overflow-auto p-8" @wheel.prevent="handleWheel">
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
        <div class="absolute left-1/2 top-1/2" :style="frameStyle">
          <img
            :src="safeSource"
            :alt="file.name"
            class="absolute left-1/2 top-1/2 max-w-none select-none object-contain shadow-lg"
            :style="imageStyle"
            draggable="false"
            @load="onImageLoad"
            @error="onImageError"
          />
        </div>
      </div>
    </div>
    <ViewerFooter>
      <template #status>
        {{ file.extension.toUpperCase() }}
        <template v-if="width"> · {{ width }} × {{ height }}</template>
        · {{ (file.size / 1024).toFixed(1) }} KiB
      </template>
      <template #actions>
        <UTooltip text="缩小" :delay-duration="500"><UButton class="viewer-footer-button" color="neutral" variant="ghost" size="xs" icon="i-lucide-zoom-out" aria-label="缩小" @click="clampZoom(zoom - 0.1)" /></UTooltip>
        <span class="w-12 text-center text-xs text-muted">{{ Math.round(zoom * 100) }}%</span>
        <UTooltip text="放大" :delay-duration="500"><UButton class="viewer-footer-button" color="neutral" variant="ghost" size="xs" icon="i-lucide-zoom-in" aria-label="放大" @click="clampZoom(zoom + 0.1)" /></UTooltip>
        <UTooltip text="全屏" :delay-duration="500"><UButton class="viewer-footer-button" color="neutral" variant="ghost" size="xs" icon="i-lucide-maximize" aria-label="全屏" @click="toggleFullscreen" /></UTooltip>
      </template>
      <template #more>
        <UPopover :content="{ side: 'top', align: 'end', sideOffset: 8 }" :ui="{ content: 'compatibility-popover' }">
          <div><UTooltip text="更多图片操作" :delay-duration="500"><UButton class="viewer-footer-button" color="neutral" variant="ghost" size="xs" icon="i-lucide-chevron-down" aria-label="更多图片操作" /></UTooltip></div>
          <template #content>
            <UButton color="neutral" variant="ghost" size="sm" icon="i-lucide-rotate-cw" label="顺时针旋转" @click="rotate" />
          </template>
        </UPopover>
      </template>
    </ViewerFooter>
  </div>
</template>
