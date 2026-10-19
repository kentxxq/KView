<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import DOMPurify from "dompurify";
import { renderAsync } from "docx-preview";
import type { OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{
  file: OpenedFile;
  content: string;
  binaryContent?: Uint8Array<ArrayBuffer>;
  readOnly: boolean;
}>();

const containerRef = ref<HTMLDivElement | null>(null);
const html = ref("");
const loading = ref(true);
const error = ref("");
let loadGeneration = 0;

const notice = computed(() =>
  props.file.extension === "doc"
    ? "当前为旧版 DOC 兼容文本模式，排版与对象可能与原文档不同。"
    : "DOCX 高保真分页预览模式；复杂宏与专属 OLE 对象不执行。",
);

function sanitizeDocumentHtml(value: string) {
  return DOMPurify.sanitize(value, {
    FORBID_TAGS: ["script", "iframe", "object", "embed", "form", "input", "button"],
    FORBID_ATTR: ["href", "srcset", "style", "onerror", "onclick"],
    ALLOW_DATA_ATTR: false,
  });
}

async function loadDocument() {
  const generation = ++loadGeneration;
  const extension = props.file.extension;
  const content = props.content;
  const bytes = props.binaryContent;

  html.value = "";
  error.value = "";
  loading.value = true;

  try {
    // 兼容历史 .doc 纯文本/HTML 预览
    if (extension === "doc") {
      if (generation === loadGeneration) {
        html.value = sanitizeDocumentHtml(content);
      }
      return;
    }

    if (!bytes || bytes.byteLength === 0) {
      throw new Error("DOCX 二进制内容不可用");
    }

    // 确保 DOM 容器已挂载
    await nextTick();
    if (!containerRef.value || generation !== loadGeneration) return;

    // 清空历史容器内容
    containerRef.value.innerHTML = "";

    // 传入独立的 slice 副本，避免 ArrayBuffer 偏移或只读视图引发的解析异常
    const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);

    await renderAsync(buffer, containerRef.value, undefined, {
      className: "docx",
      inWrapper: true,
      ignoreWidth: false,
      ignoreHeight: false,
      breakPages: true,
      experimental: false,
      trimXmlDeclaration: true,
      useBase64URL: true,
      debug: false,
    });
  } catch (cause) {
    if (generation !== loadGeneration) return;
    error.value = `无法解析 ${extension.toUpperCase()}。文件可能已损坏、加密或包含不支持的结构。${
      cause instanceof Error ? ` ${cause.message}` : ""
    }`;
  } finally {
    if (generation === loadGeneration) loading.value = false;
  }
}

onMounted(loadDocument);

watch(
  () => [props.file.path, props.file.modifiedAt, props.binaryContent],
  () => void loadDocument(),
);

onBeforeUnmount(() => {
  loadGeneration++;
  if (containerRef.value) {
    containerRef.value.innerHTML = "";
  }
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-muted/30">
    <!-- DOC 格式兼容预览 -->
    <div v-if="file.extension === 'doc'" class="min-h-0 flex-1 overflow-auto p-4 sm:p-8">
      <div v-if="loading" class="grid h-full place-items-center text-sm text-muted">
        正在解析 DOC…
      </div>
      <div v-else-if="error" class="grid h-full place-items-center p-8 text-center text-sm text-error">
        {{ error }}
      </div>
      <article
        v-else
        class="docx-preview mx-auto min-h-full max-w-4xl rounded-lg border border-muted bg-default p-8 shadow-sm sm:p-12"
        v-html="html"
      />
    </div>

    <!-- DOCX 高保真分页视口：保持 containerRef 始终挂载，避免 v-if/v-else 切换导致 ref 为 null -->
    <div v-else class="relative min-h-0 flex-1 overflow-auto p-4 sm:p-8">
      <div
        v-if="loading"
        class="absolute inset-0 z-10 grid place-items-center bg-muted/30 text-sm text-muted"
      >
        正在渲染 DOCX…
      </div>
      <div
        v-else-if="error"
        class="absolute inset-0 z-10 grid place-items-center p-8 text-center text-sm text-error"
      >
        {{ error }}
      </div>
      <div
        ref="containerRef"
        class="docx-render-host mx-auto flex w-full flex-col items-center justify-start pb-12"
      />
    </div>

    <ViewerFooter :file-size="file.size" :notice="notice">
      <template #status> {{ file.extension.toUpperCase() }} · 只读高保真预览 </template>
    </ViewerFooter>
  </div>
</template>

<style scoped>
:deep(.docx-wrapper) {
  background: transparent !important;
  padding: 0 !important;
  display: flex !important;
  flex-direction: column !important;
  align-items: center !important;
}
:deep(.docx-wrapper > section.docx) {
  box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1) !important;
  margin-bottom: 1.5rem !important;
  background-color: #ffffff !important;
  color: #1a1a1a !important;
}
</style>
