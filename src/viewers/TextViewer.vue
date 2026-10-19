<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/languages/features/json/json.worker?worker";
import "monaco-editor/features/register.all";
import type { OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";
import { resolveLanguage } from "./languages";

import "monaco-editor/languages/features/json/register";
import "monaco-editor/languages/definitions/bat/register";
import "monaco-editor/languages/definitions/cpp/register";
import "monaco-editor/languages/definitions/csharp/register";
import "monaco-editor/languages/definitions/css/register";
import "monaco-editor/languages/definitions/dart/register";
import "monaco-editor/languages/definitions/dockerfile/register";
import "monaco-editor/languages/definitions/go/register";
import "monaco-editor/languages/definitions/graphql/register";
import "monaco-editor/languages/definitions/html/register";
import "monaco-editor/languages/definitions/ini/register";
import "monaco-editor/languages/definitions/java/register";
import "monaco-editor/languages/definitions/javascript/register";
import "monaco-editor/languages/definitions/kotlin/register";
import "monaco-editor/languages/definitions/less/register";
import "monaco-editor/languages/definitions/lua/register";
import "monaco-editor/languages/definitions/markdown/register";
import "monaco-editor/languages/definitions/php/register";
import "monaco-editor/languages/definitions/powershell/register";
import "monaco-editor/languages/definitions/protobuf/register";
import "monaco-editor/languages/definitions/python/register";
import "monaco-editor/languages/definitions/ruby/register";
import "monaco-editor/languages/definitions/rust/register";
import "monaco-editor/languages/definitions/scss/register";
import "monaco-editor/languages/definitions/shell/register";
import "monaco-editor/languages/definitions/sql/register";
import "monaco-editor/languages/definitions/swift/register";
import "monaco-editor/languages/definitions/typescript/register";
import "monaco-editor/languages/definitions/xml/register";
import "monaco-editor/languages/definitions/yaml/register";

const props = withDefaults(
  defineProps<{
    file: OpenedFile;
    content: string;
    readOnly: boolean;
    embedded?: boolean;
    wordWrap?: boolean;
    unicodeHighlight?: boolean;
  }>(),
  {
    embedded: false,
  },
);
const emit = defineEmits<{
  "update:content": [value: string];
  "update:wordWrap": [value: boolean];
  "update:unicodeHighlight": [value: boolean];
}>();

const editorElement = ref<HTMLElement>();
const wordWrap = ref(props.wordWrap ?? props.file.extension !== "log");
const unicodeHighlight = ref(props.unicodeHighlight ?? false);
let editor: import("monaco-editor").editor.IStandaloneCodeEditor | undefined;
let monacoApi: typeof import("monaco-editor/editor") | undefined;
let resizeObserver: ResizeObserver | undefined;
let applyingExternalValue = false;

onMounted(async () => {
  self.MonacoEnvironment = {
    getWorker: (_, label) => (label === "json" ? new JsonWorker() : new EditorWorker()),
  };
  const monaco = await import("monaco-editor/editor");
  monacoApi = monaco;
  if (!editorElement.value) return;

  const instance = monaco.editor.create(editorElement.value, {
    value: props.content,
    language: resolveLanguage(props.file),
    theme: "vs-dark",
    readOnly: props.readOnly,
    automaticLayout: false,
    minimap: { enabled: true },
    folding: true,
    fontFamily: "Cascadia Code, SFMono-Regular, Consolas, monospace",
    fontSize: 13,
    lineHeight: 21,
    padding: { top: 12, bottom: 12 },
    scrollBeyondLastLine: false,
    smoothScrolling: true,
    wordWrap: wordWrap.value ? "on" : "off",
    unicodeHighlight: {
      ambiguousCharacters: unicodeHighlight.value,
      invisibleCharacters: unicodeHighlight.value,
      nonBasicASCII: false,
    },
  });
  editor = instance;
  instance.onDidChangeModelContent(() => {
    if (!applyingExternalValue) emit("update:content", instance.getValue());
  });
  resizeObserver = new ResizeObserver(() => editor?.layout());
  resizeObserver.observe(editorElement.value);
});

watch(
  () => props.content,
  (value) => {
    if (!editor || editor.getValue() === value) return;
    applyingExternalValue = true;
    editor.setValue(value);
    applyingExternalValue = false;
  },
);

watch(
  () => props.readOnly,
  (readOnly) => editor?.updateOptions({ readOnly }),
);

watch(wordWrap, (enabled) => {
  editor?.updateOptions({ wordWrap: enabled ? "on" : "off" });
});

watch(unicodeHighlight, (enabled) => {
  editor?.updateOptions({
    unicodeHighlight: {
      ambiguousCharacters: enabled,
      invisibleCharacters: enabled,
      nonBasicASCII: false,
    },
  });
});

watch(
  () => props.wordWrap,
  (enabled) => {
    if (enabled !== undefined) wordWrap.value = enabled;
  },
);

watch(
  () => props.unicodeHighlight,
  (enabled) => {
    if (enabled !== undefined) unicodeHighlight.value = enabled;
  },
);

watch(
  () => props.file,
  (file) => {
    const model = editor?.getModel();
    if (model) monacoApi?.editor.setModelLanguage(model, resolveLanguage(file));
    if (props.wordWrap === undefined) wordWrap.value = file.extension !== "log";
  },
);

function updateWordWrap(enabled: boolean) {
  wordWrap.value = enabled;
  emit("update:wordWrap", enabled);
}

function updateUnicodeHighlight(enabled: boolean) {
  unicodeHighlight.value = enabled;
  emit("update:unicodeHighlight", enabled);
}

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  editor?.dispose();
});
</script>

<template>
  <div v-if="embedded" ref="editorElement" class="size-full min-h-0 bg-inverted" />
  <div v-else class="flex size-full min-h-0 flex-col bg-inverted">
    <div ref="editorElement" class="min-h-0 flex-1 bg-inverted" />
    <ViewerFooter :file-size="file.size">
      <template #status>
        {{ (file.extension || "TEXT").toUpperCase() }}
        <template v-if="file.encoding"> · {{ file.encoding.toUpperCase() }}</template>
        <template v-if="file.lineEnding"> · {{ file.lineEnding.toUpperCase() }}</template>
        · {{ readOnly ? "只读" : "可编辑" }}
      </template>
      <template #actions>
        <div class="flex items-center gap-3">
          <USwitch
            :model-value="wordWrap"
            size="xs"
            label="自动换行"
            @update:model-value="updateWordWrap"
          />
          <USwitch
            :model-value="unicodeHighlight"
            size="xs"
            label="字符高亮"
            @update:model-value="updateUnicodeHighlight"
          />
        </div>
      </template>
    </ViewerFooter>
  </div>
</template>
