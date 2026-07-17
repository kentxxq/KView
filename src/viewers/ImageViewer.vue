<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import DOMPurify from "dompurify";
import type { OpenedFile } from "../types/document";
import { recordDiagnostic } from "../services/diagnostics";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();

const viewport = ref<HTMLElement>();
const zoom = ref(1);
const rotation = ref(0);
const width = ref(0);
const height = ref(0);
const failed = ref(false);
const safeSource = ref(props.content);
let objectUrl: string | undefined;

const transform = computed(
  () => `scale(${zoom.value}) rotate(${rotation.value}deg)`,
);

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

onMounted(() => {
  if (props.file.extension !== "svg") return;
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
});

onBeforeUnmount(() => {
  if (objectUrl) URL.revokeObjectURL(objectUrl);
});
</script>

<template>
  <div ref="viewport" class="relative flex size-full min-h-0 flex-col bg-muted">
    <div class="grid min-h-0 flex-1 place-items-center overflow-auto p-8" @wheel.prevent="handleWheel">
      <UAlert
        v-if="failed"
        class="max-w-md"
        color="error"
        variant="subtle"
        icon="i-lucide-image-off"
        title="图片无法显示"
        description="文件可能已损坏，或使用了当前系统 WebView 不支持的编码。"
      />
      <img
        v-else
        :src="safeSource"
        :alt="file.name"
        class="max-h-full max-w-full select-none object-contain shadow-lg transition-transform"
        :style="{ transform }"
        draggable="false"
        @load="onImageLoad"
        @error="onImageError"
      />
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
