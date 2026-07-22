<script setup lang="ts">
import { computed, defineAsyncComponent, type Component } from "vue";
import type { OpenedFile } from "../types/document";
import { resolveViewer, resolveViewerKey } from "./registry";

const props = defineProps<{
  file: OpenedFile;
  content: string;
  readOnly: boolean;
}>();

defineEmits<{ "update:content": [value: string] }>();

const viewerComponents = new Map<string, Component>();

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
    <component
      :is="viewer"
      v-if="viewer"
      class="min-h-0 flex-1"
      :file="file"
      :content="content"
      :read-only="readOnly"
      @update:content="$emit('update:content', $event)"
    />
  </div>
</template>
