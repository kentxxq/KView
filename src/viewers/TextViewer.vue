<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import "monaco-editor/min/vs/editor/editor.main.css";
import type { OpenedFile } from "../types/document";
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

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const emit = defineEmits<{ "update:content": [value: string] }>();

const editorElement = ref<HTMLElement>();
let editor: import("monaco-editor").editor.IStandaloneCodeEditor | undefined;
let resizeObserver: ResizeObserver | undefined;
let applyingExternalValue = false;

onMounted(async () => {
  self.MonacoEnvironment = {
    getWorker: (_, label) => label === "json" ? new JsonWorker() : new EditorWorker(),
  };
  const monaco = await import("monaco-editor/esm/vs/editor/editor.api.js");
  if (!editorElement.value) return;

  const instance = monaco.editor.create(editorElement.value, {
    value: props.content,
    language: resolveLanguage(props.file),
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

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  editor?.dispose();
});
</script>

<template>
  <div ref="editorElement" class="size-full min-h-0 bg-inverted" />
</template>
