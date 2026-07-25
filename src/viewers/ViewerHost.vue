<script setup lang="ts">
import { computed, defineAsyncComponent, type Component } from "vue";
import type { LargeTextSessionInfo, OpenedFile } from "../types/document";
import { resolveViewer, resolveViewerKey } from "./registry";

const props = defineProps<{
  file: OpenedFile;
  content: string;
  readOnly: boolean;
  largeTextSession: LargeTextSessionInfo | null;
}>();

defineEmits<{ "update:content": [value: string] }>();

const viewerComponents = new Map<string, Component>();
const LargeTextViewer = defineAsyncComponent(() => import("./LargeTextViewer.vue"));

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
      :content="content"
      :read-only="readOnly"
      @update:content="$emit('update:content', $event)"
    />
  </div>
</template>
