<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import DOMPurify from "dompurify";
import MarkdownIt, { type Token } from "markdown-it";
import type { ChapterItem } from "../types/chapter";
import ChapterLayout from "../components/ChapterLayout.vue";
import type { OpenedFile } from "../types/document";
import TextViewer from "./TextViewer.vue";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const emit = defineEmits<{ "update:content": [value: string] }>();
const mode = ref<"text" | "preview">("preview");
const wordWrap = ref(true);
const unicodeHighlight = ref(false);
const viewModes = [
  { label: "文本", value: "text", icon: "i-lucide-file-text" },
  { label: "预览", value: "preview", icon: "i-lucide-eye" },
];
const markdown = new MarkdownIt({ html: false, linkify: true, typographer: true });
markdown.linkify.set({ fuzzyLink: true, urlAuth: true });
const preview = ref<HTMLElement>();
const activeId = ref("");
const documentId = computed(() => JSON.stringify([props.file.path, props.file.modifiedAt]));

function headingText(tokens: Token[]): string {
  return tokens.map((token) => {
    if (token.children?.length) return headingText(token.children);
    if (["text", "code_inline"].includes(token.type)) return token.content;
    if (["softbreak", "hardbreak"].includes(token.type)) return " ";
    return "";
  }).join("");
}

const parsed = computed(() => {
  const env = {};
  const tokens = markdown.parse(props.content, env);
  const items: ChapterItem[] = [];
  const stack: Array<{ level: number; item: ChapterItem }> = [];
  const ids: string[] = [];
  tokens.forEach((token, index) => {
    if (token.type !== "heading_open") return;
    const id = `kview-heading-${ids.length}`;
    const level = Number(token.tag.slice(1));
    const item: ChapterItem = { id, label: headingText(tokens[index + 1]?.children ?? []) };
    token.attrSet("id", id);
    ids.push(id);
    while (stack.length && stack[stack.length - 1].level >= level) stack.pop();
    const parent = stack[stack.length - 1]?.item;
    if (parent) (parent.children ??= []).push(item);
    else items.push(item);
    stack.push({ level, item });
  });
  return { items, ids, html: DOMPurify.sanitize(markdown.renderer.render(tokens, markdown.options, env)) };
});

function updateActiveHeading() {
  const container = preview.value;
  if (!container) return;
  const top = container.getBoundingClientRect().top + 24;
  let current = parsed.value.ids[0] ?? "";
  for (const heading of container.querySelectorAll<HTMLElement>("h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]")) {
    if (heading.getBoundingClientRect().top > top) break;
    current = heading.id;
  }
  activeId.value = current;
}

function selectChapter(id: string) {
  const container = preview.value;
  const heading = container?.querySelector<HTMLElement>(`#${id}`);
  if (!container || !heading) return;
  container.scrollTo({ top: container.scrollTop + heading.getBoundingClientRect().top - container.getBoundingClientRect().top - 16 });
  activeId.value = id;
}

watch([parsed, mode, documentId], async (_value, oldValue) => {
  await nextTick();
  if (oldValue && oldValue[2] !== documentId.value) preview.value?.scrollTo({ top: 0 });
  updateActiveHeading();
}, { flush: "post", immediate: true });

function updateMode(value: string | number) {
  if (value === "text" || value === "preview") mode.value = value;
}

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
    <ChapterLayout
      v-else
      :items="parsed.items"
      :active-id="activeId"
      :document-id="documentId"
      @select="selectChapter"
    >
      <article
        ref="preview"
        class="markdown-preview min-h-0 w-full flex-1 overflow-auto px-8 py-10 text-default"
        v-html="parsed.html"
        @click="preventEmbeddedNavigation"
        @scroll.passive="updateActiveHeading"
        @load.capture="updateActiveHeading"
      />
    </ChapterLayout>
    <ViewerFooter :file-size="file.size">
      <template #status>
        MARKDOWN
        <template v-if="file.encoding"> · {{ file.encoding.toUpperCase() }}</template>
        <template v-if="file.lineEnding"> · {{ file.lineEnding.toUpperCase() }}</template>
      </template>
      <template #actions>
        <div v-if="mode === 'text'" class="flex items-center gap-3">
          <USwitch v-model="wordWrap" size="xs" label="自动换行" />
          <USwitch v-model="unicodeHighlight" size="xs" label="字符高亮" />
        </div>
      </template>
      <template #viewSwitch>
        <UTabs
          :model-value="mode"
          :items="viewModes"
          :content="false"
          activation-mode="automatic"
          color="neutral"
          variant="pill"
          size="xs"
          :ui="{ root: 'w-auto gap-0', list: 'w-auto p-0.5', trigger: 'py-0.5' }"
          @update:model-value="updateMode"
        />
      </template>
    </ViewerFooter>
  </div>
</template>
