<script setup lang="ts">
import { computed, defineAsyncComponent, type Component } from "vue";
import type { DocumentContent, LargeTextSessionInfo, OpenedFile } from "../types/document";
import { resolveViewer, resolveViewerKey } from "./registry";

const props = defineProps<{
  file: OpenedFile;
  content: DocumentContent;
  readOnly: boolean;
  largeTextSession: LargeTextSessionInfo | null;
}>();

defineEmits<{
  "update:content": [value: string];
  "open-hex": [];
  "pdf-modified": [bytes: Uint8Array<ArrayBuffer>];
  "save-pdf": [bytes: Uint8Array<ArrayBuffer>];
}>();

const viewerComponents = new Map<string, Component>();
const LargeTextViewer = defineAsyncComponent(() => import("./LargeTextViewer.vue"));

const textContent = computed(() => {
  if (["text", "data-url"].includes(props.content.type)) {
    return (props.content as { value: string }).value;
  }
  if (props.content.type !== "structured") return "";
  return typeof props.content.value === "string" ? props.content.value : "";
});
const structuredContent = computed(() =>
  props.content.type === "structured" ? props.content.value : undefined,
);
const binaryContent = computed(() =>
  props.content.type === "bytes" ? props.content.value : undefined,
);

const viewer = computed(() => {
  const definition = resolveViewer(props.file);
  const key = resolveViewerKey(props.file);
  if (!definition || !key) return null;
  const cached = viewerComponents.get(key);
  if (cached) return cached;
  const component = defineAsyncComponent(definition.load);
  viewerComponents.set(key, component);
  return component;
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-1">
    <LargeTextViewer
      v-if="largeTextSession"
      :key="`${file.path}:${file.modifiedAt}`"
      class="min-h-0 flex-1"
      :file="file"
      :session="largeTextSession"
    />
    <component
      :is="viewer"
      v-else-if="viewer"
      class="min-h-0 flex-1"
      :file="file"
      :content="textContent"
      :structured-content="structuredContent"
      :binary-content="binaryContent"
      :read-only="readOnly"
      @update:content="$emit('update:content', $event)"
      @open-hex="$emit('open-hex')"
      @pdf-modified="$emit('pdf-modified', $event)"
      @save-pdf="$emit('save-pdf', $event)"
    />
  </div>
</template>
