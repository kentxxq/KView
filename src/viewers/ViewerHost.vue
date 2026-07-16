<script setup lang="ts">
import { computed, defineAsyncComponent } from "vue";
import type { OpenedFile } from "../types/document";
import { resolveViewer } from "./registry";

const props = defineProps<{
  file: OpenedFile;
  content: string;
  readOnly: boolean;
}>();

defineEmits<{ "update:content": [value: string] }>();

const viewer = computed(() => {
  const definition = resolveViewer(props.file);
  return definition ? defineAsyncComponent(definition.load) : null;
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
