<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import { desktopInvoke as invoke } from "../services/desktop-api";
import DOMPurify from "dompurify";
import type { EpubChapter, EpubDocument, EpubResource, OpenedFile } from "../types/document";
import { decodeBase64 } from "../utils/binary";
import ViewerFooter from "../components/ViewerFooter.vue";
import ChapterLayout from "../components/ChapterLayout.vue";
import type { ChapterItem } from "../types/chapter";

interface EbookTocItem {
  label: string;
  href: string;
  children?: EbookTocItem[];
}

interface EbookSpineItem {
  id: string;
  href?: string;
  linear?: string;
  start?: number;
  end?: number;
}

interface EbookChapter {
  html: string;
  href?: string;
  resolvedLinks?: Array<readonly [string, EbookResolvedHref]>;
}

interface EbookResolvedHref {
  id: string;
  selector: string;
}

interface EbookSection extends EbookResolvedHref {
  label: string;
  renderSelector: string;
}

interface EbookParser {
  readonly format: "epub" | "mobi";
  getSpine(): EbookSpineItem[];
  getToc(): EbookTocItem[];
  loadChapter(id: string): EbookChapter | undefined | Promise<EbookChapter | undefined>;
  resolveHref(href: string): EbookResolvedHref | undefined;
  loadResource?(chapterId: string, href: string): Promise<EpubResource>;
  destroy(): void;
}

interface EbookWorkerResponse {
  id: number;
  result?: unknown;
  error?: string;
}

class WorkerMobiParser implements EbookParser {
  readonly format = "mobi" as const;
  private nextRequestId = 1;
  private readonly pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (cause: Error) => void }
  >();
  private spine: EbookSpineItem[] = [];
  private toc: EbookTocItem[] = [];
  private readonly resolvedLinks = new Map<string, EbookResolvedHref>();

  private constructor(private readonly worker: Worker) {
    worker.onmessage = (event: MessageEvent<EbookWorkerResponse>) => {
      const request = this.pending.get(event.data.id);
      if (!request) return;
      this.pending.delete(event.data.id);
      if (event.data.error) request.reject(new Error(event.data.error));
      else request.resolve(event.data.result);
    };
    worker.onerror = (event) => {
      const error = new Error(event.message || "电子书解析 Worker 运行失败");
      for (const request of this.pending.values()) request.reject(error);
      this.pending.clear();
    };
  }

  static async create(bytes: Uint8Array) {
    const worker = new Worker(new URL("./mobi.worker.ts", import.meta.url), { type: "module" });
    const instance = new WorkerMobiParser(worker);
    try {
      const result = await instance.request<{
        spine: EbookSpineItem[];
        toc: EbookTocItem[];
        resolvedToc: Array<readonly [string, EbookResolvedHref]>;
      }>("initialize", { buffer: bytes.buffer }, [bytes.buffer]);
      instance.spine = result.spine;
      instance.toc = result.toc;
      for (const [href, resolved] of result.resolvedToc) instance.resolvedLinks.set(href, resolved);
      return instance;
    } catch (cause) {
      worker.terminate();
      throw cause;
    }
  }

  private request<T>(
    type: string,
    payload: Record<string, unknown> = {},
    transfer: Transferable[] = [],
  ) {
    const id = this.nextRequestId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
      });
      this.worker.postMessage({ id, type, ...payload }, transfer);
    });
  }

  getSpine() {
    return this.spine;
  }

  getToc() {
    return this.toc;
  }

  async loadChapter(id: string) {
    const chapter = await this.request<EbookChapter | undefined>("load-chapter", { chapterId: id });
    for (const [href, resolved] of chapter?.resolvedLinks ?? [])
      this.resolvedLinks.set(href, resolved);
    return chapter;
  }

  resolveHref(href: string) {
    const cached = this.resolvedLinks.get(href);
    if (cached) return cached;
    const filePosition = Number(href.match(/filepos:(\d+)/)?.[1] ?? Number.NaN);
    if (!Number.isFinite(filePosition)) return undefined;
    const item = this.spine.find((entry) => entry.end !== undefined && entry.end > filePosition);
    return item ? { id: item.id, selector: `[id="filepos:${filePosition}"]` } : undefined;
  }

  destroy() {
    void this.request("destroy").then(
      () => this.worker.terminate(),
      () => this.worker.terminate(),
    );
  }
}

function fragmentSelector(fragment: string) {
  let decoded = fragment;
  try {
    decoded = decodeURIComponent(fragment);
  } catch {
    // 保留无法解码的原始锚点。
  }
  return `[id="${CSS.escape(decoded)}"]`;
}

function normalizeEpubHref(baseHref: string, href: string) {
  const value = href.trim();
  if (!value || value.startsWith("//") || /^[a-z][a-z\d+.-]*:/i.test(value)) return undefined;
  const hashIndex = value.indexOf("#");
  const fragment = hashIndex >= 0 ? value.slice(hashIndex + 1) : "";
  const withoutFragment = hashIndex >= 0 ? value.slice(0, hashIndex) : value;
  const path = withoutFragment.split("?", 1)[0];
  const combined = path
    ? path.startsWith("/")
      ? path
      : `${baseHref.slice(0, Math.max(0, baseHref.lastIndexOf("/") + 1))}${path}`
    : baseHref;
  const segments: string[] = [];
  for (const segment of combined.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      if (!segments.length) return undefined;
      segments.pop();
    } else {
      segments.push(segment);
    }
  }
  return { path: `/${segments.join("/")}`, fragment };
}

class RustEpubParser implements EbookParser {
  readonly format = "epub" as const;
  private readonly spineByHref = new Map<string, string>();
  private readonly resolvedLinks = new Map<string, EbookResolvedHref>();
  private currentChapterHref = "";

  constructor(private readonly document: EpubDocument) {
    for (const item of document.spine) this.spineByHref.set(item.href, item.id);
    const cacheToc = (items: EpubDocument["toc"]) => {
      for (const item of items) {
        if (item.href && item.chapterId) {
          this.resolvedLinks.set(item.href, {
            id: item.chapterId,
            selector: item.fragment ? fragmentSelector(item.fragment) : "",
          });
        }
        cacheToc(item.children);
      }
    };
    cacheToc(document.toc);
  }

  getSpine() {
    return this.document.spine.map((item) => ({
      id: item.id,
      href: item.href,
      linear: item.linear ? "yes" : "no",
    }));
  }

  getToc() {
    return this.document.toc;
  }

  async loadChapter(id: string): Promise<EbookChapter | undefined> {
    const chapter = await invoke<EpubChapter>("read_epub_chapter", {
      sessionId: this.document.sessionId,
      chapterId: id,
    });
    this.currentChapterHref = chapter.href;
    return { html: chapter.html, href: chapter.href };
  }

  resolveHref(href: string) {
    const cached = this.resolvedLinks.get(href);
    if (cached) return cached;
    if (!this.currentChapterHref) return undefined;
    const normalized = normalizeEpubHref(this.currentChapterHref, href);
    if (!normalized) return undefined;
    const id = this.spineByHref.get(normalized.path);
    if (!id) return undefined;
    const resolved = {
      id,
      selector: normalized.fragment ? fragmentSelector(normalized.fragment) : "",
    };
    this.resolvedLinks.set(href, resolved);
    return resolved;
  }

  async loadResource(chapterId: string, href: string) {
    return await invoke<EpubResource>("read_epub_resource", {
      sessionId: this.document.sessionId,
      chapterId,
      href,
    });
  }

  destroy() {}
}

const props = defineProps<{
  file: OpenedFile;
  content: string;
  binaryContent?: Uint8Array<ArrayBuffer>;
  readOnly: boolean;
}>();
const emit = defineEmits<{ openHex: [] }>();
const contentScroller = ref<HTMLElement>();
const chapterElement = ref<HTMLElement>();
const parser = shallowRef<EbookParser>();
const sections = ref<EbookSection[]>([]);
const toc = ref<EbookTocItem[]>([]);
const activeId = ref("");
const chapterDirectory = computed(() => {
  const targets = new Map<string, string>();
  function mapItems(items: EbookTocItem[], prefix = "ebook"): ChapterItem[] {
    return items.map((item, index) => {
      const id = `${prefix}-${index}`;
      targets.set(id, item.href);
      return { id, label: item.label, children: mapItems(item.children ?? [], id) };
    });
  }
  return { items: mapItems(toc.value), targets };
});
const chapterIndex = ref(0);
const chapterHtml = ref("");
const fontSize = ref(100);
const loading = ref(true);
const error = ref("");
let loadGeneration = 0;
let chapterGeneration = 0;
let chapterObjectUrls: string[] = [];

function revokeChapterObjectUrls() {
  for (const url of chapterObjectUrls) URL.revokeObjectURL(url);
  chapterObjectUrls = [];
}

function flattenToc(items: EbookTocItem[], result: EbookTocItem[] = []) {
  for (const item of items) {
    result.push(item);
    flattenToc(item.children ?? [], result);
  }
  return result;
}

function createSections(
  currentParser: EbookParser,
  spineItems: EbookSpineItem[],
  tocItems: EbookTocItem[],
) {
  const seen = new Set<string>();
  const tocSections: EbookSection[] = [];
  for (const item of flattenToc(tocItems)) {
    const resolved = currentParser.resolveHref(item.href);
    if (!resolved) continue;
    const key = `${resolved.id}\0${resolved.selector}`;
    if (seen.has(key)) continue;
    seen.add(key);
    tocSections.push({ ...resolved, renderSelector: resolved.selector, label: item.label });
  }
  const useTocSections =
    props.file.extension === "mobi" ||
    (tocSections.length > spineItems.length && tocSections.some((section) => section.selector));
  if (!useTocSections || !tocSections.length) {
    return spineItems.map((item) => ({ id: item.id, selector: "", renderSelector: "", label: "" }));
  }
  if (props.file.extension === "mobi") {
    tocSections.forEach((section, index) => {
      section.renderSelector = `[data-kview-ebook-section="${index}"]`;
    });
  }
  return tocSections;
}

function extractSectionHtml(value: string, startSelector: string, endSelector: string) {
  if (!startSelector) return value;
  const template = document.createElement("template");
  template.innerHTML = value;
  try {
    const start = template.content.querySelector(startSelector);
    if (!start) return value;
    const range = document.createRange();
    range.selectNodeContents(template.content);
    range.setStartBefore(start);
    if (endSelector) {
      const end = template.content.querySelector(endSelector);
      if (end) range.setEndBefore(end);
    }
    const container = document.createElement("div");
    container.append(range.cloneContents());
    for (const marker of container.querySelectorAll("[data-kview-ebook-section]")) marker.remove();
    return container.innerHTML;
  } catch {
    return value;
  }
}

function sanitizeChapter(value: string) {
  const sanitized = DOMPurify.sanitize(value, {
    FORBID_TAGS: [
      "script",
      "iframe",
      "object",
      "embed",
      "form",
      "input",
      "button",
      "textarea",
      "select",
      "style",
      "link",
      "meta",
      "base",
      "svg",
      "math",
      "video",
      "audio",
      "source",
      "track",
      "canvas",
    ],
    FORBID_ATTR: ["style", "srcset", "formaction", "ping"],
    ALLOW_DATA_ATTR: false,
    ALLOWED_URI_REGEXP:
      /^(?:(?:blob|epub|filepos|kindle):|data:image\/(?:png|jpeg|gif|webp|bmp|avif);base64,|(?![a-z][a-z\d+.-]*:|\/\/).+)/i,
  });
  const template = document.createElement("template");
  template.innerHTML = sanitized;

  for (const image of template.content.querySelectorAll("img")) {
    const source = image.getAttribute("src") ?? "";
    if (!/^(?:blob:|data:image\/(?:png|jpeg|gif|webp|bmp|avif);base64,)/i.test(source)) {
      image.remove();
      continue;
    }
    image.removeAttribute("srcset");
  }
  for (const anchor of template.content.querySelectorAll("a")) {
    const href = anchor.getAttribute("href") ?? "";
    if (!href || !parser.value?.resolveHref(href)) {
      anchor.removeAttribute("href");
    }
    anchor.removeAttribute("target");
    anchor.removeAttribute("rel");
  }
  return template.innerHTML;
}

async function createParser(bytes: Uint8Array): Promise<EbookParser> {
  if (props.file.extension === "epub") {
    const document = JSON.parse(props.content) as EpubDocument;
    if (!document.sessionId || !Array.isArray(document.spine) || !Array.isArray(document.toc)) {
      throw new Error("EPUB 会话数据无效");
    }
    return new RustEpubParser(document);
  }
  return await WorkerMobiParser.create(bytes);
}

async function hydrateChapterImages(value: string, chapterId: string, currentParser: EbookParser) {
  if (!currentParser.loadResource) return { html: value, objectUrls: [] as string[] };
  const template = document.createElement("template");
  template.innerHTML = value;
  const images = [...template.content.querySelectorAll("img[src]")];
  const sources = [
    ...new Set(images.map((image) => image.getAttribute("src") ?? "").filter(Boolean)),
  ];
  const loaded = await Promise.all(
    sources.map(async (source) => {
      if (source.startsWith("data:") || source.startsWith("blob:"))
        return [source, source] as const;
      try {
        const resource = await currentParser.loadResource?.(chapterId, source);
        if (!resource) return [source, ""] as const;
        const bytes = decodeBase64(resource.base64);
        const objectUrl = URL.createObjectURL(new Blob([bytes], { type: resource.mimeType }));
        return [source, objectUrl] as const;
      } catch {
        return [source, ""] as const;
      }
    }),
  );
  const replacements = new Map(loaded);
  for (const image of images) {
    const source = image.getAttribute("src") ?? "";
    const replacement = replacements.get(source);
    if (!replacement) image.remove();
    else image.setAttribute("src", replacement);
  }
  return {
    html: template.innerHTML,
    objectUrls: loaded.flatMap(([, url]) => (url.startsWith("blob:") ? [url] : [])),
  };
}

function findActiveChapter(section: EbookSection, selector: string) {
  let first = "";
  for (const [id, href] of chapterDirectory.value.targets) {
    const resolved = parser.value?.resolveHref(href);
    if (resolved?.id !== section.id) continue;
    if (!first) first = id;
    if (!section.renderSelector && !selector) return first;
    if (resolved.selector === (section.renderSelector || selector || section.selector)) return id;
  }
  return first;
}

function selectChapter(id: string) {
  const href = chapterDirectory.value.targets.get(id);
  if (href) void navigateHref(href, id);
}

async function showChapter(index: number, selector = "", selectedId?: string) {
  const generation = ++chapterGeneration;
  const currentParser = parser.value;
  const item = sections.value[index];
  if (!currentParser || !item) return;
  error.value = "";
  loading.value = true;
  try {
    const chapter = await currentParser.loadChapter(item.id);
    if (parser.value !== currentParser) return;
    if (!chapter) throw new Error("章节内容不存在");
    const targetSelector = item.renderSelector || selector || item.selector;
    const nextItem = sections.value[index + 1];
    const endSelector =
      item.renderSelector && nextItem?.id === item.id ? nextItem.renderSelector : "";
    const sectionHtml = item.renderSelector
      ? extractSectionHtml(chapter.html, targetSelector, endSelector)
      : chapter.html;
    const hydrated = await hydrateChapterImages(sectionHtml, item.id, currentParser);
    if (generation !== chapterGeneration || parser.value !== currentParser) {
      for (const url of hydrated.objectUrls) URL.revokeObjectURL(url);
      return;
    }
    revokeChapterObjectUrls();
    chapterObjectUrls = hydrated.objectUrls;
    chapterIndex.value = index;
    activeId.value = selectedId ?? findActiveChapter(item, selector);
    chapterHtml.value = sanitizeChapter(hydrated.html);
    loading.value = false;
    await nextTick();
    if (!item.renderSelector && targetSelector) {
      try {
        chapterElement.value?.querySelector(targetSelector)?.scrollIntoView({ block: "start" });
      } catch {
        contentScroller.value?.scrollTo({ top: 0 });
      }
    } else {
      contentScroller.value?.scrollTo({ top: 0 });
    }
  } catch (cause) {
    error.value = `无法读取电子书章节。${cause instanceof Error ? ` ${cause.message}` : ""}`;
    loading.value = false;
  }
}

async function navigateHref(href: string, selectedId?: string) {
  const resolved = parser.value?.resolveHref(href);
  if (!resolved) return;
  const sectionedNavigation = sections.value.some((item) => item.renderSelector);
  const index = sectionedNavigation
    ? sections.value.findIndex(
        (item) => item.id === resolved.id && item.selector === resolved.selector,
      )
    : sections.value.findIndex((item) => item.id === resolved.id);
  if (index >= 0) await showChapter(index, resolved.selector, selectedId);
}

function handleChapterClick(event: MouseEvent) {
  const target = event.target instanceof Element ? event.target.closest("a") : null;
  if (!target) return;
  event.preventDefault();
  const href = target.getAttribute("href");
  if (href) void navigateHref(href);
}

async function initialize() {
  const generation = ++loadGeneration;
  chapterGeneration += 1;
  revokeChapterObjectUrls();
  parser.value?.destroy();
  parser.value = undefined;
  sections.value = [];
  toc.value = [];
  activeId.value = "";
  chapterHtml.value = "";
  error.value = "";
  loading.value = true;
  try {
    const nextParser = await createParser(
      props.file.extension === "epub"
        ? new Uint8Array()
        : (props.binaryContent?.slice() ??
            (() => {
              throw new Error("MOBI 二进制内容不可用");
            })()),
    );
    if (generation !== loadGeneration) {
      nextParser.destroy();
      return;
    }
    parser.value = nextParser;
    const allSpineItems = nextParser.getSpine();
    let readableSpineItems = allSpineItems.filter((item) => item.linear !== "no");
    if (!readableSpineItems.length) readableSpineItems = allSpineItems;
    toc.value = nextParser.getToc() ?? [];
    sections.value = createSections(nextParser, readableSpineItems, toc.value);
    if (!sections.value.length) throw new Error("电子书不包含可阅读章节");
    const initialIndex = sections.value.some((section) => section.label)
      ? Math.max(
          0,
          sections.value.findIndex(
            (section) => !/^(?:目录|contents|table of contents)$/i.test(section.label.trim()),
          ),
        )
      : 0;
    await showChapter(initialIndex);
  } catch (cause) {
    if (generation !== loadGeneration) return;
    error.value = `无法解析 ${props.file.extension.toUpperCase()}。文件可能已损坏、加密、受 DRM 保护或包含不支持的结构。${cause instanceof Error ? ` ${cause.message}` : ""}`;
    loading.value = false;
  }
}

watch(
  () => [props.file.path, props.file.modifiedAt],
  () => void initialize(),
  { immediate: true },
);

onBeforeUnmount(() => {
  loadGeneration += 1;
  chapterGeneration += 1;
  revokeChapterObjectUrls();
  parser.value?.destroy();
  parser.value = undefined;
});
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-muted/20">
    <ChapterLayout :items="chapterDirectory.items" :active-id="activeId"
      :document-id="JSON.stringify([file.path, file.modifiedAt])"
      :loading="loading && !toc.length" @select="selectChapter">

      <section class="flex min-h-0 min-w-0 flex-1 flex-col">
        <div v-if="loading" class="grid flex-1 place-items-center text-sm text-muted">
          正在解析电子书…
        </div>
        <div v-else-if="error" class="grid flex-1 place-items-center p-8 text-center">
          <div>
            <p class="text-sm text-error">{{ error }}</p>
            <UButton
              class="mt-4"
              color="neutral"
              variant="soft"
              size="sm"
              icon="i-lucide-binary"
              label="以十六进制查看"
              @click="emit('openHex')"
            />
          </div>
        </div>
        <div v-else ref="contentScroller" class="min-h-0 flex-1 overflow-auto p-4 sm:p-8">
          <article
            ref="chapterElement"
            class="ebook-preview mx-auto min-h-full max-w-3xl rounded-lg border border-muted bg-default p-7 shadow-sm sm:p-12"
            :style="{ fontSize: `${fontSize}%` }"
            @click="handleChapterClick"
            v-html="chapterHtml"
          />
        </div>
      </section>
    </ChapterLayout>

    <ViewerFooter
      :file-size="file.size"
      notice="仅提供安全的基础排版预览；电子书脚本、外部资源和自定义样式不会加载。"
    >
      <template #status
        >{{ file.extension.toUpperCase() }} · {{ chapterIndex + 1 }}/{{ sections.length || "-" }} 章
        · 只读</template
      >
      <template #actions>
        <UTooltip text="上一章" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-chevron-left"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="上一章"
            :disabled="chapterIndex <= 0 || loading"
            @click="showChapter(chapterIndex - 1)"
        /></UTooltip>
        <UTooltip text="下一章" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-chevron-right"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="下一章"
            :disabled="chapterIndex >= sections.length - 1 || loading"
            @click="showChapter(chapterIndex + 1)"
        /></UTooltip>
        <UTooltip text="缩小字号" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-minus"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="缩小字号"
            :disabled="fontSize <= 80"
            @click="fontSize = Math.max(80, fontSize - 10)"
        /></UTooltip>
        <span class="w-12 text-center text-xs text-muted">{{ fontSize }}%</span>
        <UTooltip text="放大字号" :delay-duration="500"
          ><UButton
            class="viewer-footer-button"
            icon="i-lucide-plus"
            color="neutral"
            variant="ghost"
            size="xs"
            aria-label="放大字号"
            :disabled="fontSize >= 180"
            @click="fontSize = Math.min(180, fontSize + 10)"
        /></UTooltip>
      </template>
    </ViewerFooter>
  </div>
</template>
