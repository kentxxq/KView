<script setup lang="ts">
import { computed, ref } from "vue";
import DOMPurify from "dompurify";
import MarkdownIt from "markdown-it";
import type { OpenedFile } from "../types/document";
import TextViewer from "./TextViewer.vue";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const emit = defineEmits<{ "update:content": [value: string] }>();
const mode = ref<"text" | "preview">("preview");
const wordWrap = ref(true);
const unicodeHighlight = ref(false);
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
      v-if="mode === 'text'"
      class="min-h-0 flex-1"
      :file="file"
      :content="content"
      :read-only="readOnly"
      v-model:word-wrap="wordWrap"
      v-model:unicode-highlight="unicodeHighlight"
      embedded
      @update:content="emit('update:content', $event)"
    />
    <article
      v-else
      class="markdown-preview w-full flex-1 overflow-auto px-8 py-10 text-default"
      v-html="rendered"
      @click="preventEmbeddedNavigation"
    />
    <ViewerFooter :file-size="file.size">
      <template #status>
        MARKDOWN
        <template v-if="file.encoding"> · {{ file.encoding.toUpperCase() }}</template>
        <template v-if="file.lineEnding"> · {{ file.lineEnding.toUpperCase() }}</template>
      </template>
      <template #actions>
        <UButton class="viewer-footer-button transition-colors hover:bg-accented hover:text-highlighted" color="neutral" size="xs" icon="i-lucide-file-text" label="文本" :variant="mode === 'text' ? 'soft' : 'ghost'" @click="mode = 'text'" />
        <UButton class="viewer-footer-button transition-colors hover:bg-accented hover:text-highlighted" color="neutral" size="xs" icon="i-lucide-eye" label="预览" :variant="mode === 'preview' ? 'soft' : 'ghost'" @click="mode = 'preview'" />
        <div v-if="mode === 'text'" class="ml-1 flex items-center gap-3 border-l border-muted/70 pl-2">
          <USwitch v-model="wordWrap" size="xs" label="自动换行" />
          <USwitch v-model="unicodeHighlight" size="xs" label="字符高亮" />
        </div>
      </template>
    </ViewerFooter>
  </div>
</template>
