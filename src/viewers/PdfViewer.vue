<script setup lang="ts">
import { computed, nextTick, onActivated, onDeactivated, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { useToast } from "@nuxt/ui/composables";
import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
import type { AnnotationEditorUIManager } from "pdfjs-dist/types/src/display/editor/tools";
import { EventBus, PDFLinkService, PDFViewer, ScrollMode } from "pdfjs-dist/web/pdf_viewer.mjs";
import "pdfjs-dist/web/pdf_viewer.css";
import { getPdfAnnotationSession } from "../services/pdf-annotations";
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
const emit = defineEmits<{
  "pdf-modified": [bytes: Uint8Array<ArrayBuffer>];
  "save-pdf": [bytes: Uint8Array<ArrayBuffer>];
}>();
const toast = useToast();
const container = ref<HTMLDivElement>();
const viewerElement = ref<HTMLDivElement>();
const editorMode = ref(pdfjs.AnnotationEditorType.NONE);
const editorReady = ref(false);
const canUndo = ref(false);
const canRedo = ref(false);
const hasSelection = ref(false);
const editorColors = ref({ highlight: "#ffeb3b", ink: "#e53935", text: "#e53935" });
const colorKey = computed(() => editorMode.value === pdfjs.AnnotationEditorType.HIGHLIGHT ? "highlight"
  : editorMode.value === pdfjs.AnnotationEditorType.INK ? "ink" : "text");
const editorColor = computed({
  get: () => editorColors.value[colorKey.value],
  set: (value: string) => { editorColors.value[colorKey.value] = value; },
});
const tools = [
  { mode: pdfjs.AnnotationEditorType.NONE, label: "选择", icon: "i-lucide-mouse-pointer-2" },
  { mode: pdfjs.AnnotationEditorType.HIGHLIGHT, label: "高亮", icon: "i-lucide-highlighter" },
  { mode: pdfjs.AnnotationEditorType.INK, label: "画笔", icon: "i-lucide-pencil" },
  { mode: pdfjs.AnnotationEditorType.FREETEXT, label: "文字", icon: "i-lucide-type" },
];
let pdfViewer: PDFViewer | undefined;
let eventBus: EventBus | undefined;
let viewerAbort: AbortController | undefined;
let loadedBytes: Uint8Array<ArrayBuffer> | undefined;
let releaseViewer: (() => Promise<void>) | undefined;
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
let loadingTask: PDFDocumentLoadingTask | undefined;
let loadGeneration = 0;

async function renderPage() {
  const currentDocument = document.value;
  const currentViewer = pdfViewer;
  if (!currentDocument || !currentViewer) return;
  currentViewer.currentPageNumber = pageNumber.value;
  currentViewer.currentScale = scale.value;
  const page = await currentDocument.getPage(pageNumber.value);
  const text = await page.getTextContent();
  if (document.value !== currentDocument || page.pageNumber !== pageNumber.value) return;
  pageText.value = text.items.map((item) => ("str" in item ? item.str : "")).join(" ");
}

function selectTool(mode: number) {
  if (!pdfViewer || !editorReady.value) return;
  editorMode.value = mode;
  pdfViewer.annotationEditorMode = { mode };
}

function updateColor() {
  if (editorMode.value === pdfjs.AnnotationEditorType.NONE) return;
  const params = pdfjs.AnnotationEditorParamsType;
  const type = editorMode.value === pdfjs.AnnotationEditorType.HIGHLIGHT ? params.HIGHLIGHT_COLOR
    : editorMode.value === pdfjs.AnnotationEditorType.INK ? params.INK_COLOR : params.FREETEXT_COLOR;
  eventBus?.dispatch("switchannotationeditorparams", { source: pdfViewer, type, value: editorColor.value });
}

function editingAction(name: "undo" | "redo" | "delete") {
  eventBus?.dispatch("editingaction", { source: pdfViewer, name });
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
  const previousRelease = releaseViewer;
  releaseViewer = undefined;
  const previousLoadingTask = loadingTask;
  loadingTask = undefined;
  if (previousRelease) await previousRelease();
  else if (previousLoadingTask) await previousLoadingTask.destroy();
  loadedBytes = undefined;
  pdfViewer = undefined;
  if (generation !== loadGeneration) return;
  document.value = undefined;
  pageNumber.value = 1;
  pageCount.value = 0;
  outline.value = [];
  outlineTargets.clear();
  activeId.value = "";
  error.value = "";
  loading.value = true;
  editorReady.value = false;
  editorMode.value = pdfjs.AnnotationEditorType.NONE;
  canUndo.value = canRedo.value = hasSelection.value = false;
  try {
    const bytes = props.binaryContent;
    if (!bytes) throw new Error("PDF 二进制内容不可用");
    const session = getPdfAnnotationSession(bytes);
    const restored = await session.exportBytes();
    if (generation !== loadGeneration) return;
    const task = pdfjs.getDocument({ data: restored.slice() });
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
    if (generation !== loadGeneration) return;
    if (!container.value || !viewerElement.value) throw new Error("PDF 页面容器不可用");
    const bus = new EventBus();
    eventBus = bus;
    const linkService = new PDFLinkService({ eventBus: bus });
    const abort = new AbortController();
    viewerAbort = abort;
    const options: ConstructorParameters<typeof PDFViewer>[0] & { abortSignal: AbortSignal } = {
      container: container.value,
      viewer: viewerElement.value,
      eventBus: bus,
      linkService,
      annotationEditorMode: pdfjs.AnnotationEditorType.NONE,
      annotationEditorHighlightColors: "yellow=#FFEB3B,green=#81C784,blue=#64B5F6,pink=#F48FB1,red=#E53935",
      annotationMode: pdfjs.AnnotationMode.ENABLE,
      enablePermissions: true,
      enableAutoLinking: false,
      abortSignal: abort.signal,
    };
    const instance = new PDFViewer(options);
    let uiManager: AnnotationEditorUIManager | undefined;
    bus.on("annotationeditoruimanager", ({ uiManager: manager }: { uiManager: AnnotationEditorUIManager }) => { uiManager = manager; });
    pdfViewer = instance;
    instance.scrollMode = ScrollMode.PAGE;
    linkService.setViewer(instance);
    linkService.setDocument(nextDocument);
    bus.on("annotationeditorlayerrendered", () => {
      // 此版本声明把 getter 误写成 setter 的参数类型，运行时返回数值模式。
      editorReady.value = !nextDocument.isPureXfa && Number(instance.annotationEditorMode) !== pdfjs.AnnotationEditorType.DISABLE;
    });
    bus.on("annotationeditormodechanged", ({ mode }: { mode: number }) => {
      editorMode.value = mode;
      // 工具类型异步初始化完成后再设置颜色，避免被引擎默认值覆盖。
      updateColor();
    });
    bus.on("editingstateschanged", ({ details }: { details: { hasSomethingToUndo: boolean; hasSomethingToRedo: boolean; hasSelectedEditor: boolean } }) => {
      canUndo.value = details.hasSomethingToUndo;
      canRedo.value = details.hasSomethingToRedo;
      hasSelection.value = details.hasSelectedEditor;
    });
    bus.on("pagechanging", ({ pageNumber: page }: { pageNumber: number }) => { pageNumber.value = page; });
    bus.on("pagerendered", ({ error: cause }: { error?: Error }) => {
      if (cause) error.value = `PDF 页面渲染失败：${cause.message}`;
    });
    // PDF.js 的声明将回调推断为 null，实际接口允许设置函数。
    const storage = nextDocument.annotationStorage as unknown as { onSetModified: (() => void) | null };
    storage.onSetModified = () => emit("pdf-modified", bytes);
    const exportBytes = async () => {
      uiManager?.commitOrRemove();
      return nextDocument.saveDocument();
    };
    session.exportBytes = exportBytes;
    loadedBytes = bytes;
    releaseViewer = async () => {
      const snapshot = exportBytes();
      session.exportBytes = () => snapshot;
      storage.onSetModified = null;
      try {
        await snapshot;
      } finally {
        instance.setDocument(null);
        abort.abort();
        await task.destroy();
      }
    };
    instance.setDocument(nextDocument);
    await instance.firstPagePromise;
    if (generation !== loadGeneration) return;
    await renderPage();
  } catch (cause) {
    if (generation !== loadGeneration) return;
    error.value = `无法解析 PDF。文件可能已损坏、加密或使用了不支持的特性。${cause instanceof Error ? ` ${cause.message}` : ""}`;
  } finally {
    if (generation === loadGeneration) loading.value = false;
  }
}

onMounted(loadPdf);
onActivated(() => pdfViewer?.update());
onDeactivated(() => {
  // 退出编辑模式，避免后台标签继续处理全局键盘和粘贴事件。
  if (editorReady.value) selectTool(pdfjs.AnnotationEditorType.NONE);
});
watch(
  () => [props.file.path, props.file.modifiedAt],
  () => void loadPdf(),
);

watch(pageNumber, () => { activeId.value = ""; }, { flush: "sync" });
watch([pageNumber, scale], () => void nextTick(renderPage));
onBeforeUnmount(() => {
  loadGeneration++;
  const cleanup = releaseViewer ? releaseViewer() : loadingTask?.destroy();
  void cleanup?.catch((cause: unknown) => {
    toast.add({ title: "无法保留 PDF 标注", description: String(cause), color: "error" });
  });
  viewerAbort?.abort();
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-muted/20">
    <ChapterLayout :items="outline" :active-id="activeId"
      :document-id="JSON.stringify([file.path, file.modifiedAt])" :loading="loading" @select="goToOutline">
      <section class="flex min-h-0 min-w-0 flex-1 flex-col">
        <div v-if="!loading && !error" class="flex flex-wrap items-center gap-1 border-b border-default bg-default px-3 py-2">
          <UButton v-for="tool in tools" :key="tool.mode" :icon="tool.icon" :label="tool.label"
            size="xs" :color="editorMode === tool.mode ? 'primary' : 'neutral'"
            :variant="editorMode === tool.mode ? 'soft' : 'ghost'" :disabled="!editorReady"
            :aria-pressed="editorMode === tool.mode" @click="selectTool(tool.mode)" />
          <label class="mx-2 flex items-center gap-1 text-xs text-muted">
            颜色<input v-model="editorColor" type="color" aria-label="标注颜色" class="h-6 w-7 cursor-pointer"
              :disabled="!editorReady || editorMode === pdfjs.AnnotationEditorType.NONE" @input="updateColor" />
          </label>
          <UButton icon="i-lucide-undo-2" aria-label="撤销标注" size="xs" color="neutral" variant="ghost" :disabled="!canUndo" @click="editingAction('undo')" />
          <UButton icon="i-lucide-redo-2" aria-label="重做标注" size="xs" color="neutral" variant="ghost" :disabled="!canRedo" @click="editingAction('redo')" />
          <UButton icon="i-lucide-trash-2" aria-label="删除所选标注" size="xs" color="neutral" variant="ghost" :disabled="!hasSelection" @click="editingAction('delete')" />
          <UButton class="ml-auto" icon="i-lucide-save" label="另存为 PDF" size="xs" variant="soft" :disabled="!editorReady" @click="loadedBytes && emit('save-pdf', loadedBytes)" />
        </div>
        <div v-if="loading" class="grid flex-1 place-items-center text-sm text-muted">
          正在加载 PDF…
        </div>
        <div
          v-else-if="error"
          class="grid flex-1 place-items-center p-8 text-center text-sm text-error"
        >
          {{ error }}
        </div>
        <div v-show="!loading && !error" class="relative min-h-0 flex-1">
          <div ref="container" class="kview-pdf-container absolute inset-0 overflow-auto">
            <div ref="viewerElement" class="pdfViewer" />
          </div>
        </div>
      </section>
    </ChapterLayout>
    <ViewerFooter :file-size="file.size">
      <template #status>PDF · {{ pageNumber }}/{{ pageCount || "-" }} · {{ editorReady ? '支持标注 · 原文件只读' : '只读' }}</template>
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

<style>
.kview-pdf-container {
  color-scheme: light;
  color: #111;
}
.kview-pdf-container .pdfViewer {
  padding: 20px;
}
.kview-pdf-container .pdfViewer .page {
  margin: 0 auto;
}
</style>
