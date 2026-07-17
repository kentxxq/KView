<script setup lang="ts">
import { computed, ref } from "vue";
import DOMPurify from "dompurify";
import MarkdownIt from "markdown-it";
import type { OpenedFile } from "../types/document";
import TextViewer from "./TextViewer.vue";
import ViewerFooter from "../components/ViewerFooter.vue";

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
    <TextViewer
      v-if="mode === 'source'"
      class="min-h-0 flex-1"
      :file="file"
      :content="content"
      :read-only="readOnly"
      embedded
      @update:content="emit('update:content', $event)"
    />
    <article
      v-else
      class="markdown-preview mx-auto w-full max-w-4xl flex-1 overflow-auto px-8 py-10 text-default"
      v-html="rendered"
      @click="preventEmbeddedNavigation"
    />
    <ViewerFooter>
      <template #status>
        MARKDOWN
        <template v-if="file.encoding"> · {{ file.encoding.toUpperCase() }}</template>
        <template v-if="file.lineEnding"> · {{ file.lineEnding.toUpperCase() }}</template>
      </template>
      <template #actions>
        <UButton class="viewer-footer-button" color="neutral" size="xs" icon="i-lucide-code-xml" label="源码" :variant="mode === 'source' ? 'soft' : 'ghost'" @click="mode = 'source'" />
        <UButton class="viewer-footer-button" color="neutral" size="xs" icon="i-lucide-eye" label="预览" :variant="mode === 'preview' ? 'soft' : 'ghost'" @click="mode = 'preview'" />
      </template>
    </ViewerFooter>
  </div>
</template>
