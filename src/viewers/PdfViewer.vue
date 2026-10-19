<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
import type { OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";
import ChapterLayout from "../components/ChapterLayout.vue";
import type { ChapterItem } from "../types/chapter";

pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  "pdfjs-dist/build/pdf.worker.min.mjs",
  import.meta.url,
).href;

const props = defineProps<{
  file: OpenedFile;
  content: string;
  binaryContent?: Uint8Array<ArrayBuffer>;
  readOnly: boolean;
}>();
const canvas = ref<HTMLCanvasElement>();
const document = shallowRef<PDFDocumentProxy>();
const pageNumber = ref(1);
const pageCount = ref(0);
const scale = ref(1.1);
const loading = ref(true);
const error = ref("");
const outline = ref<ChapterItem[]>([]);
const outlineTargets = new Map<string, unknown>();
const activeId = ref("");
let navigationGeneration = 0;
const query = ref("");
const matches = ref<number[]>([]);
const pageText = ref("");
let renderTask: { cancel: () => void; promise: Promise<unknown> } | undefined;
let loadingTask: PDFDocumentLoadingTask | undefined;
let loadGeneration = 0;

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
  pageText.value = text.items.map((item) => ("str" in item ? item.str : "")).join(" ");
}

async function search() {
  matches.value = [];
  const term = query.value.trim().toLocaleLowerCase();
  if (!term || !document.value) return;
  for (let index = 1; index <= document.value.numPages; index += 1) {
    const page = await document.value.getPage(index);
    const content = await page.getTextContent();
    const text = content.items
      .map((item) => ("str" in item ? item.str : ""))
      .join(" ")
      .toLocaleLowerCase();
    if (text.includes(term)) matches.value.push(index);
  }
  if (matches.value.length) pageNumber.value = matches.value[0];
}

function mapOutline(entries: NonNullable<Awaited<ReturnType<PDFDocumentProxy["getOutline"]>>>, prefix = "pdf"): ChapterItem[] {
  return entries.map((entry, index) => {
    const id = `${prefix}-${index}`;
    outlineTargets.set(id, entry.dest);
    return { id, label: entry.title, children: mapOutline(entry.items ?? [], id) };
  });
}

async function goToOutline(id: string) {
  const currentDocument = document.value;
  const destination = outlineTargets.get(id);
  if (!currentDocument || !destination) return;
  const generation = ++navigationGeneration;
  try {
    const resolved = typeof destination === "string"
      ? await currentDocument.getDestination(destination) : destination;
    if (!Array.isArray(resolved) || resolved[0] == null) return;
    const index = typeof resolved[0] === "number"
      ? resolved[0] : await currentDocument.getPageIndex(resolved[0]);
    if (document.value !== currentDocument || generation !== navigationGeneration) return;
    if (!Number.isInteger(index) || index < 0 || index >= currentDocument.numPages) return;
    pageNumber.value = index + 1;
    activeId.value = id;
  } catch {
    // 无效的大纲目标不影响正文阅读。
  }
}

async function copyPageText() {
  await navigator.clipboard.writeText(pageText.value);
}

async function loadPdf() {
  const generation = ++loadGeneration;
  navigationGeneration++;
  renderTask?.cancel();
  const previousLoadingTask = loadingTask;
  loadingTask = undefined;
  await previousLoadingTask?.destroy().catch(() => undefined);
  if (generation !== loadGeneration) return;
  document.value = undefined;
  pageNumber.value = 1;
  pageCount.value = 0;
  outline.value = [];
  outlineTargets.clear();
  activeId.value = "";
  error.value = "";
  loading.value = true;
  try {
    const bytes = props.binaryContent;
    if (!bytes) throw new Error("PDF 二进制内容不可用");
    const task = pdfjs.getDocument({ data: bytes.slice() });
    loadingTask = task;
    const nextDocument = await task.promise;
    if (generation !== loadGeneration) {
      await task.destroy();
      return;
    }
    document.value = nextDocument;
    pageCount.value = nextDocument.numPages;
    const entries = await nextDocument.getOutline();
    if (generation !== loadGeneration) return;
    outline.value = mapOutline(entries ?? []);
    loading.value = false;
    await nextTick();
    await renderPage();
  } catch (cause) {
    if (generation !== loadGeneration) return;
    error.value = `无法解析 PDF。文件可能已损坏、加密或使用了不支持的特性。${cause instanceof Error ? ` ${cause.message}` : ""}`;
  } finally {
    if (generation === loadGeneration) loading.value = false;
  }
}

onMounted(loadPdf);
watch(
  () => [props.file.path, props.file.modifiedAt],
  () => void loadPdf(),
);

watch(pageNumber, () => { activeId.value = ""; }, { flush: "sync" });
watch([pageNumber, scale], () => void nextTick(renderPage));
onBeforeUnmount(() => {
  loadGeneration++;
  renderTask?.cancel();
  void loadingTask?.destroy();
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-muted/20">
    <ChapterLayout :items="outline" :active-id="activeId"
      :document-id="JSON.stringify([file.path, file.modifiedAt])" :loading="loading" @select="goToOutline">
      <section class="flex min-h-0 min-w-0 flex-1 flex-col">
        <div v-if="loading" class="grid flex-1 place-items-center text-sm text-muted">
          正在加载 PDF…
        </div>
        <div
          v-else-if="error"
          class="grid flex-1 place-items-center p-8 text-center text-sm text-error"
        >
          {{ error }}
        </div>
        <div v-else class="min-h-0 flex-1 overflow-auto p-5 text-center">
          <canvas ref="canvas" class="mx-auto bg-white shadow-lg" />
        </div>
      </section>
    </ChapterLayout>
    <ViewerFooter :file-size="file.size">
      <template #status>PDF · {{ pageNumber }}/{{ pageCount || "-" }} · 只读</template>
      <template #actions>
        <UTooltip text="上一页" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-chevron-left"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="上一页"
            :disabled="pageNumber <= 1"
            @click="pageNumber--"
        /></UTooltip>
        <UTooltip text="下一页" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-chevron-right"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="下一页"
            :disabled="pageNumber >= pageCount"
            @click="pageNumber++"
        /></UTooltip>
        <UTooltip text="缩小" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-zoom-out"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="缩小"
            :disabled="scale <= 0.6"
            @click="scale = Math.max(0.5, scale - 0.2)"
        /></UTooltip>
        <span class="w-12 text-center text-xs text-muted">{{ Math.round(scale * 100) }}%</span>
        <UTooltip text="放大" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-zoom-in"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="放大"
            :disabled="scale >= 3"
            @click="scale = Math.min(3, scale + 0.2)"
        /></UTooltip>
        <UPopover
          :content="{ side: 'top', align: 'end', sideOffset: 8 }"
          :ui="{ content: 'compatibility-popover' }"
        >
          <UButton
            class="viewer-footer-button"
            color="neutral"
            variant="ghost"
            size="xs"
            icon="i-lucide-search"
            label="搜索"
          />
          <template #content>
            <div class="w-80 space-y-2">
              <div class="flex items-center gap-2">
                <UInput
                  v-model="query"
                  class="min-w-0 flex-1"
                  icon="i-lucide-search"
                  size="sm"
                  placeholder="搜索 PDF"
                  @keyup.enter="search"
                />
                <UButton color="neutral" variant="soft" size="sm" label="搜索" @click="search" />
              </div>
              <span class="text-xs text-muted">{{
                query ? `找到 ${matches.length} 页` : "输入关键词搜索全文"
              }}</span>
            </div>
          </template>
        </UPopover>
        <UTooltip text="复制本页文本" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-copy"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="复制本页文本"
            :disabled="!pageText"
            @click="copyPageText"
        /></UTooltip>
      </template>
    </ViewerFooter>
  </div>
</template>
