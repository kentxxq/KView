<script setup lang="ts">
import { computed, ref } from "vue";
import DOMPurify from "dompurify";
import MarkdownIt from "markdown-it";
import type { OpenedFile } from "../types/document";
import TextViewer from "./TextViewer.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const emit = defineEmits<{ "update:content": [value: string] }>();
const mode = ref<"source" | "preview">("preview");
const markdown = new MarkdownIt({ html: false, linkify: true, typographer: true });
const rendered = computed(() => DOMPurify.sanitize(markdown.render(props.content)));

function preventEmbeddedNavigation(event: MouseEvent) {
  const link = (event.target as HTMLElement).closest("a");
  if (link) event.preventDefault();
}
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-default">
    <div class="flex h-12 shrink-0 items-center gap-1 border-b border-muted px-4">
      <UButton
        color="neutral"
        :variant="mode === 'source' ? 'soft' : 'ghost'"
        icon="i-lucide-code-xml"
        label="源码"
        @click="mode = 'source'"
      />
      <UButton
        color="neutral"
        :variant="mode === 'preview' ? 'soft' : 'ghost'"
        icon="i-lucide-eye"
        label="预览"
        @click="mode = 'preview'"
      />
    </div>

    <TextViewer
      v-if="mode === 'source'"
      class="min-h-0 flex-1"
      :file="file"
      :content="content"
      :read-only="readOnly"
      @update:content="emit('update:content', $event)"
    />
    <article
      v-else
      class="markdown-preview mx-auto w-full max-w-4xl flex-1 overflow-auto px-8 py-10 text-default"
      v-html="rendered"
      @click="preventEmbeddedNavigation"
    />
  </div>
</template>
