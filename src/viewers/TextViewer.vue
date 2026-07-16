<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import "monaco-editor/min/vs/editor/editor.main.css";
import type { OpenedFile } from "../types/document";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const emit = defineEmits<{ "update:content": [value: string] }>();

const editorElement = ref<HTMLElement>();
let editor: import("monaco-editor").editor.IStandaloneCodeEditor | undefined;
let resizeObserver: ResizeObserver | undefined;
let applyingExternalValue = false;

const languageByExtension: Record<string, string> = {
  json: "json",
  yaml: "yaml",
  yml: "yaml",
  md: "markdown",
  markdown: "markdown",
  xml: "xml",
  toml: "ini",
  ini: "ini",
  env: "ini",
  conf: "ini",
  sql: "sql",
  log: "plaintext",
  txt: "plaintext",
};

onMounted(async () => {
  self.MonacoEnvironment = { getWorker: () => new EditorWorker() };
  const monaco = await import("monaco-editor/esm/vs/editor/editor.api");
  if (!editorElement.value) return;

  editor = monaco.editor.create(editorElement.value, {
    value: props.content,
    language: languageByExtension[props.file.extension] ?? "plaintext",
    theme: "vs-dark",
    readOnly: props.readOnly,
    automaticLayout: false,
    minimap: { enabled: false },
    folding: true,
    fontFamily: "Cascadia Code, SFMono-Regular, Consolas, monospace",
    fontSize: 13,
    lineHeight: 21,
    padding: { top: 12, bottom: 12 },
    scrollBeyondLastLine: false,
    smoothScrolling: true,
    wordWrap: props.file.extension === "log" ? "off" : "on",
  });
  editor.onDidChangeModelContent(() => {
    if (!applyingExternalValue) emit("update:content", editor?.getValue() ?? "");
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

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  editor?.dispose();
});
</script>

<template>
  <div ref="editorElement" class="size-full min-h-0 bg-inverted" />
</template>
