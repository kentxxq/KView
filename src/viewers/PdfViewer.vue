<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentProxy } from "pdfjs-dist";
import type { OpenedFile } from "../types/document";
import { decodeBase64 } from "../utils/binary";

pdfjs.GlobalWorkerOptions.workerSrc = new URL("pdfjs-dist/build/pdf.worker.min.mjs", import.meta.url).href;

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const canvas = ref<HTMLCanvasElement>();
const document = shallowRef<PDFDocumentProxy>();
const pageNumber = ref(1);
const pageCount = ref(0);
const scale = ref(1.1);
const loading = ref(true);
const error = ref("");
const outline = ref<Array<{ title: string; destination: unknown }>>([]);
const query = ref("");
const matches = ref<number[]>([]);
const pageText = ref("");
let renderTask: { cancel: () => void; promise: Promise<unknown> } | undefined;

async function renderPage() {
  if (!document.value || !canvas.value) return;
  renderTask?.cancel();
  const page = await document.value.getPage(pageNumber.value);
  const viewport = page.getViewport({ scale: scale.value });
  const context = canvas.value.getContext("2d");
  if (!context) return;
  canvas.value.width = Math.ceil(viewport.width);
  canvas.value.height = Math.ceil(viewport.height);
  const currentTask = page.render({ canvas: canvas.value, canvasContext: context, viewport });
  renderTask = currentTask;
  await currentTask.promise.catch((cause: unknown) => {
    if (!(cause instanceof Error) || cause.name !== "RenderingCancelledException") throw cause;
  });
  const text = await page.getTextContent();
  pageText.value = text.items.map((item) => "str" in item ? item.str : "").join(" ");
}

async function search() {
  matches.value = [];
  const term = query.value.trim().toLocaleLowerCase();
  if (!term || !document.value) return;
  for (let index = 1; index <= document.value.numPages; index += 1) {
    const page = await document.value.getPage(index);
    const content = await page.getTextContent();
    const text = content.items.map((item) => "str" in item ? item.str : "").join(" ").toLocaleLowerCase();
    if (text.includes(term)) matches.value.push(index);
  }
  if (matches.value.length) pageNumber.value = matches.value[0];
}

async function goToOutline(destination: unknown) {
  if (!document.value || !destination) return;
  const resolved = typeof destination === "string" ? await document.value.getDestination(destination) : destination;
  if (!Array.isArray(resolved)) return;
  const index = await document.value.getPageIndex(resolved[0]);
  pageNumber.value = index + 1;
}

async function copyPageText() {
  await navigator.clipboard.writeText(pageText.value);
}

onMounted(async () => {
  try {
    const task = pdfjs.getDocument({ data: decodeBase64(props.content) });
    document.value = await task.promise;
    pageCount.value = document.value.numPages;
    const entries = await document.value.getOutline();
    outline.value = (entries ?? []).map((entry) => ({ title: entry.title, destination: entry.dest }));
    loading.value = false;
    await nextTick();
    await renderPage();
  } catch (cause) {
    error.value = `无法解析 PDF。文件可能已损坏、加密或使用了不支持的特性。${cause instanceof Error ? ` ${cause.message}` : ""}`;
  } finally {
    loading.value = false;
  }
});

watch([pageNumber, scale], () => void nextTick(renderPage));
onBeforeUnmount(() => {
  renderTask?.cancel();
  void document.value?.destroy();
});
</script>

<template>
  <div class="flex size-full min-h-0 bg-muted/20">
    <aside class="hidden w-56 shrink-0 overflow-auto border-r border-muted bg-default p-3 lg:block">
      <p class="mb-2 text-xs font-semibold uppercase tracking-wide text-muted">文档目录</p>
      <button v-for="entry in outline" :key="entry.title" class="mb-1 block w-full rounded px-2 py-1.5 text-left text-sm hover:bg-elevated" @click="goToOutline(entry.destination)">{{ entry.title }}</button>
      <p v-if="!outline.length" class="text-xs leading-5 text-muted">此 PDF 未提供目录。</p>
    </aside>
    <section class="flex min-w-0 flex-1 flex-col">
      <div class="flex flex-wrap items-center gap-2 border-b border-muted bg-default px-3 py-2">
        <UButton icon="i-lucide-chevron-left" color="neutral" variant="ghost" aria-label="上一页" :disabled="pageNumber <= 1" @click="pageNumber--" />
        <span class="text-sm">{{ pageNumber }} / {{ pageCount || "-" }}</span>
        <UButton icon="i-lucide-chevron-right" color="neutral" variant="ghost" aria-label="下一页" :disabled="pageNumber >= pageCount" @click="pageNumber++" />
        <UButton icon="i-lucide-zoom-out" color="neutral" variant="ghost" aria-label="缩小" :disabled="scale <= 0.6" @click="scale = Math.max(0.5, scale - 0.2)" />
        <span class="w-12 text-center text-xs text-muted">{{ Math.round(scale * 100) }}%</span>
        <UButton icon="i-lucide-zoom-in" color="neutral" variant="ghost" aria-label="放大" :disabled="scale >= 3" @click="scale = Math.min(3, scale + 0.2)" />
        <div class="ml-auto flex items-center gap-2">
          <UInput v-model="query" icon="i-lucide-search" size="sm" placeholder="搜索 PDF" @keyup.enter="search" />
          <UButton color="neutral" variant="soft" size="sm" label="搜索" @click="search" />
          <UBadge v-if="query" color="neutral" variant="subtle">{{ matches.length }} 页</UBadge>
          <UButton icon="i-lucide-copy" color="neutral" variant="ghost" size="sm" label="复制本页文本" :disabled="!pageText" @click="copyPageText" />
        </div>
      </div>
      <div v-if="loading" class="grid flex-1 place-items-center text-sm text-muted">正在加载 PDF…</div>
      <div v-else-if="error" class="grid flex-1 place-items-center p-8 text-center text-sm text-error">{{ error }}</div>
      <div v-else class="min-h-0 flex-1 overflow-auto p-5 text-center">
        <canvas ref="canvas" class="mx-auto bg-white shadow-lg" />
      </div>
    </section>
  </div>
</template>
