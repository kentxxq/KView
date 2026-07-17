<script setup lang="ts">
import { onMounted, ref } from "vue";
import DOMPurify from "dompurify";
import * as mammoth from "mammoth";
import type { OpenedFile } from "../types/document";
import { decodeBase64 } from "../utils/binary";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const html = ref("");
const loading = ref(true);
const error = ref("");
const warnings = ref<string[]>([]);

function sanitizeDocumentHtml(value: string) {
  return DOMPurify.sanitize(value, {
    FORBID_TAGS: ["script", "iframe", "object", "embed", "form", "input", "button"],
    FORBID_ATTR: ["href", "srcset", "style", "onerror", "onclick"],
    ALLOW_DATA_ATTR: false,
  });
}

onMounted(async () => {
  try {
    if (props.file.extension === "doc") {
      html.value = sanitizeDocumentHtml(props.content);
      return;
    }
    const bytes = decodeBase64(props.content);
    const result = await mammoth.convertToHtml(
      { arrayBuffer: bytes.buffer as ArrayBuffer },
      {
        convertImage: mammoth.images.imgElement(async (image) => ({
          src: `data:${image.contentType};base64,${await image.read("base64")}`,
        })),
      },
    );
    html.value = sanitizeDocumentHtml(result.value);
    warnings.value = result.messages.map((message) => message.message).slice(0, 3);
  } catch (cause) {
    error.value = `无法解析 ${props.file.extension.toUpperCase()}。文件可能已损坏、加密或包含不支持的结构。${cause instanceof Error ? ` ${cause.message}` : ""}`;
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-muted/30">
    <div v-if="loading" class="grid flex-1 place-items-center text-sm text-muted">正在解析 DOCX…</div>
    <div v-else-if="error" class="grid flex-1 place-items-center p-8 text-center text-sm text-error">{{ error }}</div>
    <div v-else class="min-h-0 flex-1 overflow-auto p-4 sm:p-8">
      <article class="docx-preview mx-auto min-h-full max-w-4xl rounded-lg border border-muted bg-default p-8 shadow-sm sm:p-12" v-html="html" />
      <p v-if="warnings.length" class="mx-auto mt-3 max-w-4xl text-xs text-muted">解析提示：{{ warnings.join("；") }}</p>
    </div>
    <ViewerFooter>
      <template #status>
        <span class="inline-flex items-center gap-1">
          <span>{{ file.extension.toUpperCase() }} · 只读兼容预览</span>
          <UTooltip :delay-duration="500" :content="{ side: 'top', align: 'start', sideOffset: 8 }" :ui="{ content: 'compatibility-popover' }" arrow>
            <UIcon name="i-lucide-info" class="size-3.5 text-amber-500" />
            <template #content>
              <p class="max-w-sm text-xs leading-5 text-default">复杂分页、浮动对象、宏和 Office 专有排版可能与原文档不同；KView 不会执行文档链接或脚本。</p>
            </template>
          </UTooltip>
        </span>
      </template>
    </ViewerFooter>
  </div>
</template>
