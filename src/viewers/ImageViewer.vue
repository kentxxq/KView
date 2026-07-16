<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import DOMPurify from "dompurify";
import type { OpenedFile } from "../types/document";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();

const viewport = ref<HTMLElement>();
const zoom = ref(1);
const rotation = ref(0);
const width = ref(0);
const height = ref(0);
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

async function toggleFullscreen() {
  if (!document.fullscreenElement) await viewport.value?.requestFullscreen();
  else await document.exitFullscreen();
}

function onImageLoad(event: Event) {
  const image = event.currentTarget as HTMLImageElement;
  width.value = image.naturalWidth;
  height.value = image.naturalHeight;
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
    <div class="flex h-12 shrink-0 items-center justify-between border-b border-muted bg-default px-4">
      <div class="flex items-center gap-1">
        <UButton color="neutral" variant="ghost" icon="i-lucide-zoom-out" @click="clampZoom(zoom - 0.1)" />
        <span class="w-14 text-center text-xs text-muted">{{ Math.round(zoom * 100) }}%</span>
        <UButton color="neutral" variant="ghost" icon="i-lucide-zoom-in" @click="clampZoom(zoom + 0.1)" />
        <UButton color="neutral" variant="ghost" icon="i-lucide-rotate-cw" @click="rotate" />
      </div>
      <div class="flex items-center gap-3 text-xs text-muted">
        <span v-if="width">{{ width }} × {{ height }}</span>
        <span>{{ file.extension.toUpperCase() }} · {{ (file.size / 1024).toFixed(1) }} KiB</span>
        <UButton color="neutral" variant="ghost" icon="i-lucide-maximize" @click="toggleFullscreen" />
      </div>
    </div>

    <div class="grid min-h-0 flex-1 place-items-center overflow-auto p-8">
      <img
        :src="safeSource"
        :alt="file.name"
        class="max-h-full max-w-full select-none object-contain shadow-lg transition-transform"
        :style="{ transform }"
        draggable="false"
        @load="onImageLoad"
      />
    </div>
  </div>
</template>
