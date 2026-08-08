<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import "monaco-editor/min/vs/editor/editor.main.css";
import type { OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";
import { resolveLanguage } from "./languages";

import "monaco-editor/esm/vs/language/json/monaco.contribution.js";
import "monaco-editor/esm/vs/basic-languages/bat/bat.contribution.js";
import "monaco-editor/esm/vs/basic-languages/cpp/cpp.contribution.js";
import "monaco-editor/esm/vs/basic-languages/csharp/csharp.contribution.js";
import "monaco-editor/esm/vs/basic-languages/css/css.contribution.js";
import "monaco-editor/esm/vs/basic-languages/dart/dart.contribution.js";
import "monaco-editor/esm/vs/basic-languages/dockerfile/dockerfile.contribution.js";
import "monaco-editor/esm/vs/basic-languages/go/go.contribution.js";
import "monaco-editor/esm/vs/basic-languages/graphql/graphql.contribution.js";
import "monaco-editor/esm/vs/basic-languages/html/html.contribution.js";
import "monaco-editor/esm/vs/basic-languages/ini/ini.contribution.js";
import "monaco-editor/esm/vs/basic-languages/java/java.contribution.js";
import "monaco-editor/esm/vs/basic-languages/javascript/javascript.contribution.js";
import "monaco-editor/esm/vs/basic-languages/kotlin/kotlin.contribution.js";
import "monaco-editor/esm/vs/basic-languages/less/less.contribution.js";
import "monaco-editor/esm/vs/basic-languages/lua/lua.contribution.js";
import "monaco-editor/esm/vs/basic-languages/markdown/markdown.contribution.js";
import "monaco-editor/esm/vs/basic-languages/php/php.contribution.js";
import "monaco-editor/esm/vs/basic-languages/powershell/powershell.contribution.js";
import "monaco-editor/esm/vs/basic-languages/protobuf/protobuf.contribution.js";
import "monaco-editor/esm/vs/basic-languages/python/python.contribution.js";
import "monaco-editor/esm/vs/basic-languages/ruby/ruby.contribution.js";
import "monaco-editor/esm/vs/basic-languages/rust/rust.contribution.js";
import "monaco-editor/esm/vs/basic-languages/scss/scss.contribution.js";
import "monaco-editor/esm/vs/basic-languages/shell/shell.contribution.js";
import "monaco-editor/esm/vs/basic-languages/sql/sql.contribution.js";
import "monaco-editor/esm/vs/basic-languages/swift/swift.contribution.js";
import "monaco-editor/esm/vs/basic-languages/typescript/typescript.contribution.js";
import "monaco-editor/esm/vs/basic-languages/xml/xml.contribution.js";
import "monaco-editor/esm/vs/basic-languages/yaml/yaml.contribution.js";

const props = withDefaults(defineProps<{
  file: OpenedFile;
  content: string;
  readOnly: boolean;
  embedded?: boolean;
  wordWrap?: boolean;
  unicodeHighlight?: boolean;
}>(), {
  embedded: false,
});
const emit = defineEmits<{
  "update:content": [value: string];
  "update:wordWrap": [value: boolean];
  "update:unicodeHighlight": [value: boolean];
}>();

const editorElement = ref<HTMLElement>();
const wordWrap = ref(props.wordWrap ?? props.file.extension !== "log");
const unicodeHighlight = ref(props.unicodeHighlight ?? false);
let editor: import("monaco-editor").editor.IStandaloneCodeEditor | undefined;
let monacoApi: typeof import("monaco-editor/esm/vs/editor/editor.api.js") | undefined;
let resizeObserver: ResizeObserver | undefined;
let applyingExternalValue = false;

onMounted(async () => {
  self.MonacoEnvironment = {
    getWorker: (_, label) => label === "json" ? new JsonWorker() : new EditorWorker(),
  };
  const monaco = await import("monaco-editor/esm/vs/editor/editor.api.js");
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
