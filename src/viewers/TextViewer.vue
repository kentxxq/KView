<script setup lang="ts">
import { inject, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";
import { resolveLanguage } from "./languages";
import { textDocumentKey, textModelFor } from "../services/text-models";
import { loadMonaco } from "../services/monaco-runtime";
import { errorMessage } from "../utils/error";

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
const loadError = ref("");
const wordWrap = ref(props.wordWrap ?? props.file.extension !== "log");
const unicodeHighlight = ref(props.unicodeHighlight ?? false);
let editor: import("monaco-editor").editor.IStandaloneCodeEditor | undefined;
let monacoApi: typeof import("monaco-editor/editor") | undefined;
let resizeObserver: ResizeObserver | undefined;
let applyingExternalValue = false;
let disposed = false;
const document = inject(textDocumentKey);

onMounted(async () => {
  try {
  const monaco = await loadMonaco();
  monacoApi = monaco;
  if (!editorElement.value || disposed) return;
  const model = document ? await textModelFor(document) : monaco.editor.createModel(props.content, resolveLanguage(props.file));
  if (disposed) { if (!document) model.dispose(); return; }

  const instance = monaco.editor.create(editorElement.value, {
    model,
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
    if (!document && !applyingExternalValue) emit("update:content", instance.getValue());
  });
  resizeObserver = new ResizeObserver(() => editor?.layout());
  resizeObserver.observe(editorElement.value);
  } catch (cause) { if (!disposed) loadError.value = errorMessage(cause); }
});

watch(
  () => props.content,
  (value) => {
    if (!editor || document || editor.getValue() === value) return;
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
  async (file) => {
    try {
    if (document && editor && document.content.type === "text") {
      const shared = await textModelFor(document);
      if (!disposed) editor.setModel(shared);
    }
    const model = editor?.getModel();
    if (model) monacoApi?.editor.setModelLanguage(model, resolveLanguage(file));
    if (props.wordWrap === undefined) wordWrap.value = file.extension !== "log";
    } catch (cause) { if (!disposed) loadError.value = errorMessage(cause); }
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
  disposed = true;
  resizeObserver?.disconnect();
  if (!document) editor?.getModel()?.dispose();
  editor?.dispose();
});
</script>

<template>
  <UAlert v-if="loadError" color="error" title="无法载入文本编辑器" :description="loadError" />
  <div v-else-if="embedded" ref="editorElement" class="size-full min-h-0 bg-inverted" />
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
