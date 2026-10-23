<script setup lang="ts">
import { computed, nextTick, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, shallowRef, watch } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useToast } from "@nuxt/ui/composables";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { DocumentState, OpenedFile, TextEncoding } from "../types/document";
import { desktopInvoke } from "../services/desktop-api";
import { registerLogSession, unregisterLogSession } from "../services/document-resources";
import { useSettingsStore, type LogHighlightRule } from "../state/settings";
import { errorMessage } from "../utils/error";
import { recordDiagnostic } from "../services/diagnostics";
import ViewerFooter from "../components/ViewerFooter.vue";
import { createLogLayout, type LogRow } from "../services/log-layout";

interface LogChunk { content: string; encoding: TextEncoding; truncated: boolean }
const props = defineProps<{ file: OpenedFile; document: DocumentState }>();
const settings = useSettingsStore();
const toast = useToast();
const viewport = ref<HTMLElement>();
const rows = shallowRef<LogRow[]>([]);
const scrollTop = ref(0);
const height = ref(600);
const width = ref(800);
const scrollLeft = ref(0);
const layoutVersion = ref(0);
let layout: ReturnType<typeof createLogLayout> | undefined;
let layoutFrame: number | undefined;
let removeFontListener: (() => void) | undefined;
const following = ref(true);
const error = ref("");
let lineTruncationNotified = false;
let readingPositionLost = false;
const loading = ref(false);
const rulesOpen = ref(false);
const rules = ref<LogHighlightRule[]>([]);
const rulesSaving = ref(false);
const rulesError = ref("");
const colorItems = [{ label: "红", value: "red" }, { label: "橙", value: "orange" }, { label: "黄", value: "yellow" }, { label: "绿", value: "green" }, { label: "蓝", value: "blue" }, { label: "紫", value: "purple" }];
const ROW_HEIGHT = 21;
const LIMIT = 10 * 1024 * 1024;
let sessionId: string | undefined;
let sequence = 0;
let rowSequence = 0;
let bufferBytes = 0;
let encoding: TextEncoding = "utf-8";
let active = true;
let disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;
let busy = false;
let resize: ResizeObserver | undefined;
let unlisten: UnlistenFn | undefined;
const start = computed(() => Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - 12));
const end = computed(() => Math.min(rows.value.length, start.value + Math.ceil(height.value / ROW_HEIGHT) + 24));
const visibleRows = computed(() => rows.value.slice(start.value, end.value));
const encoder = new TextEncoder();
function bytesFor(text: string) { return encoding.startsWith("utf-16") ? text.length * 2 : encoder.encode(text).length; }

const renderedRows = computed(() => {
  layoutVersion.value;
  return visibleRows.value.map((row) => ({ row, segments: layout?.render(row, scrollLeft.value, Math.max(0, width.value - 80), settings.logHighlightRules) ?? [] }));
});
const indexedWidth = computed(() => { layoutVersion.value; return layout?.width() ?? 0; });
const layoutProgress = computed(() => { layoutVersion.value; return layout?.progress(visibleRows.value) ?? 1; });

function stopLayout() {
  if (layoutFrame !== undefined) cancelAnimationFrame(layoutFrame);
  layoutFrame = undefined;
}
function scheduleLayout() {
  if (!layout || disposed || !active || layoutFrame !== undefined) return;
  layoutFrame = requestAnimationFrame(() => {
    layoutFrame = undefined;
    if (!layout || disposed || !active) return;
    try {
      layout.advance(visibleRows.value, performance.now() + 4);
      layoutVersion.value++;
      if (layout.progress(visibleRows.value) < 1) scheduleLayout();
    } catch (cause) { error.value = errorMessage(cause); }
  });
}
watch(visibleRows, scheduleLayout);

async function append(chunk: LogChunk, reset = false) {
  encoding = chunk.encoding;
  const current = reset ? [] : rows.value.slice();
  if (reset) { bufferBytes = 0; rowSequence = 0; layout?.clear(); }
  const previous = current.pop();
  if (previous) bufferBytes -= previous.bytes;
  const tail = (previous?.raw ?? "") + chunk.content;
  let at = 0;
  let id = previous?.id;
  let removed = 0;
  let head = 0;
  let lineTruncated = false;
  function push(raw: string) {
    let bytes = bytesFor(raw);
    let trimmed = false;
    if (bytes > LIMIT) {
      let cut = Math.max(1, Math.floor(raw.length * (bytes - LIMIT) / bytes));
      if (cut < raw.length && /[\uDC00-\uDFFF]/.test(raw[cut]!)) cut++;
      let kept = raw.slice(cut);
      while (bytesFor(kept) > LIMIT) {
        cut += Math.max(1, Math.ceil((bytesFor(kept) - LIMIT) / 2));
        if (cut < raw.length && /[\uDC00-\uDFFF]/.test(raw[cut]!)) cut++;
        kept = raw.slice(cut);
      }
      raw = kept;
      bytes = bytesFor(raw);
      lineTruncated = true;
      trimmed = true;
    }
    current.push({ id: id ?? rowSequence++, raw, bytes, trimmed });
    id = undefined;
    bufferBytes += bytes;
    while (current.length - head > 1 && (bufferBytes > LIMIT || current.length - head > 50_000)) {
      bufferBytes -= current[head++]!.bytes;
      removed++;
    }
    if (head >= 4096) { current.splice(0, head); head = 0; }
  }
  for (const match of tail.matchAll(/\r\n|\n|\r(?!$)/g)) {
    const to = match.index + match[0].length;
    push(tail.slice(at, to));
    at = to;
  }
  push(tail.slice(at));
  if (head) current.splice(0, head);
  const oldTop = active ? viewport.value?.scrollTop ?? 0 : scrollTop.value;
  if (lineTruncated && !lineTruncationNotified) {
    lineTruncationNotified = true;
    toast.add({ title: "单行超过缓冲区上限", description: "已截断该行较早内容，仅保留最近内容。", color: "warning", duration: 4000 });
  }
  if (!following.value && !readingPositionLost && (removed * ROW_HEIGHT > oldTop || lineTruncated && oldTop < ROW_HEIGHT)) {
    readingPositionLost = true;
    toast.add({ title: "原阅读位置已移出缓冲区", description: "已保留当前可用的最早内容，可点击继续滚动回到末尾。", color: "warning", duration: 4000 });
  }
  rows.value = current;
  layout?.retain(current);
  layoutVersion.value++;
  scheduleLayout();
  await nextTick();
  if (!viewport.value || disposed) return;
  if (!active) { scrollTop.value = Math.max(0, oldTop - removed * ROW_HEIGHT); return; }
  viewport.value.scrollTop = following.value ? viewport.value.scrollHeight : Math.max(0, oldTop - removed * ROW_HEIGHT);
  scrollTop.value = viewport.value.scrollTop;
}

function stop() { if (timer) clearTimeout(timer); timer = undefined; }
function resumePolling() {
  stop();
  if (active && !disposed && !busy && !loading.value && !error.value && sessionId) void poll();
}
async function closeSession() {
  const id = sessionId;
  sessionId = undefined;
  if (!id) return;
  unregisterLogSession(props.document, id);
  await desktopInvoke("close_log_session", { sessionId: id });
}
async function visible() {
  if (!active || disposed) return false;
  return !isTauri() || await getCurrentWindow().isVisible() && !await getCurrentWindow().isMinimized();
}
async function poll() {
  if (disposed || !active || busy || error.value) return;
  busy = true;
  const generation = sequence;
  try {
    if (sessionId && await visible()) {
      const chunk = await desktopInvoke<LogChunk>("read_log_increment", { sessionId });
      if (generation === sequence && !disposed && chunk.content) await append(chunk);
      else if (generation === sequence && following.value && viewport.value && active) {
        viewport.value.scrollTop = viewport.value.scrollHeight;
        scrollTop.value = viewport.value.scrollTop;
      }
    }
  } catch (cause) { if (!disposed && generation === sequence) error.value = errorMessage(cause); }
  finally {
    busy = false;
    if (!disposed && active && !error.value) timer = setTimeout(() => void poll(), 500);
  }
}
async function reconnect() {
  if (loading.value || disposed) return;
  const generation = ++sequence;
  stop();
  loading.value = true;
  error.value = "";
  try {
    await closeSession();
    const windowGeneration = await desktopInvoke<string>("get_log_window_generation");
    if (disposed || generation !== sequence) return;
    const result = await desktopInvoke<{ sessionId: string; chunk: LogChunk }>("open_log_session", { path: props.file.path, windowGeneration });
    if (disposed || generation !== sequence) { await desktopInvoke("close_log_session", { sessionId: result.sessionId }); return; }
    sessionId = result.sessionId;
    registerLogSession(props.document, sessionId);
    following.value = true;
    lineTruncationNotified = false;
    readingPositionLost = false;
    await append(result.chunk, true);
  } catch (cause) { if (!disposed) error.value = errorMessage(cause); }
  finally { loading.value = false; resumePolling(); }
}
function onScroll() {
  if (!viewport.value) return;
  const previous = scrollTop.value;
  scrollTop.value = viewport.value.scrollTop;
  scrollLeft.value = viewport.value.scrollLeft;
  if (scrollTop.value < previous - 1 && viewport.value.scrollTop + viewport.value.clientHeight < viewport.value.scrollHeight - ROW_HEIGHT) following.value = false;
}
async function toggleFollowing() {
  following.value = !following.value;
  if (following.value) readingPositionLost = false;
  if (following.value && viewport.value) { await nextTick(); viewport.value.scrollTop = viewport.value.scrollHeight; scrollTop.value = viewport.value.scrollTop; }
}
function editRules() { rules.value = settings.logHighlightRules.map((rule) => ({ ...rule })); rulesError.value = ""; rulesOpen.value = true; }
async function saveRules() {
  rulesSaving.value = true;
  rulesError.value = "";
  try { await settings.save({ theme: settings.theme, residencyMode: settings.residencyMode, closeBehavior: settings.closeBehavior, autostart: settings.autostart, largeFileThresholdMb: settings.largeFileThresholdMb, logHighlightRules: rules.value }); rulesOpen.value = false; }
  catch (cause) { rulesError.value = errorMessage(cause); }
  finally { rulesSaving.value = false; }
}
onMounted(async () => {
  try {
  const context = document.createElement("canvas").getContext("2d");
  if (!context) throw new Error("无法创建日志字体测量上下文");
  layout = createLogLayout(context);
  const refreshLayout = (force = false) => {
    if (!viewport.value || disposed || !layout) return;
    height.value = viewport.value.clientHeight;
    width.value = viewport.value.clientWidth;
    if (layout.configure(getComputedStyle(viewport.value), force)) layoutVersion.value++;
    scheduleLayout();
  };
  if (viewport.value) { resize = new ResizeObserver(() => refreshLayout()); resize.observe(viewport.value); }
  const fontLoaded = () => refreshLayout(true);
  document.fonts.addEventListener("loadingdone", fontLoaded);
  removeFontListener = () => document.fonts.removeEventListener("loadingdone", fontLoaded);
  await document.fonts.ready;
  refreshLayout();
  if (isTauri()) {
    const dispose = await getCurrentWindow().onFocusChanged(resumePolling);
    if (disposed) dispose(); else unlisten = dispose;
  }
  if (!disposed) await reconnect();
  } catch (cause) { if (!disposed) error.value = errorMessage(cause); }
});
onActivated(async () => {
  active = true;
  await nextTick();
  if (disposed || !active) return;
  if (viewport.value) viewport.value.scrollTop = following.value ? viewport.value.scrollHeight : scrollTop.value;
  scheduleLayout();
  resumePolling();
});
onDeactivated(() => { active = false; stop(); stopLayout(); });
onBeforeUnmount(() => { disposed = true; sequence++; stop(); stopLayout(); layout?.clear(); resize?.disconnect(); removeFontListener?.(); unlisten?.(); void closeSession().catch(() => recordDiagnostic("viewer", "log-close-failed")); });
</script>

<template>
  <section class="flex size-full min-h-0 flex-col bg-inverted text-white/90">
    <UAlert v-if="error" class="m-3 shrink-0" color="error" title="日志跟随已停止" :description="error" />
    <div ref="viewport" class="min-h-0 flex-1 overflow-auto font-mono text-[13px]" style="overflow-anchor: none; font-kerning: none; font-variant-ligatures: none; letter-spacing: 0; tab-size: 8" @scroll.passive="onScroll">
      <div :style="{ height: `${rows.length * ROW_HEIGHT}px`, position: 'relative', minWidth: '100%', width: `${indexedWidth + 96}px` }">
        <div :style="{ transform: `translateY(${start * ROW_HEIGHT}px)` }" class="absolute left-0 top-0 min-w-full">
          <div v-for="item in renderedRows" :key="item.row.id" class="relative whitespace-pre" :style="{ height: `${ROW_HEIGHT}px`, lineHeight: `${ROW_HEIGHT}px` }">
            <span class="sticky left-0 z-10 inline-block w-20 shrink-0 select-none bg-inverted pr-3 text-right text-white/35">{{ item.row.id + 1 }}</span>
            <span v-for="segment in item.segments" :key="segment.start" class="absolute top-0" :style="{ left: `${80 + segment.left}px`, width: `${segment.width}px` }"><span v-for="(part, index) in segment.parts" :key="index" :style="{ color: part.color, display: part.tabWidth === undefined ? undefined : 'inline-block', width: part.tabWidth === undefined ? undefined : `${part.tabWidth}px`, overflow: part.tabWidth === undefined ? undefined : 'hidden' }">{{ part.text }}</span></span>
          </div>
        </div>
      </div>
    </div>
    <ViewerFooter :notice="['只读日志 · 最多保留最近 10 MiB、50,000 行内容，不包含完整历史']">
      <template #status>{{ encoding.toUpperCase() }} · {{ loading ? '连接中' : following ? '自动滚动' : '滚动已暂停' }}<span v-if="layoutProgress < 1"> · 长行布局 {{ (layoutProgress * 100).toFixed(0) }}%</span></template>
      <template #actions>
        <UButton size="xs" variant="ghost" @click="editRules">关键词着色</UButton>
        <UButton v-if="error" size="xs" variant="ghost" :loading="loading" @click="reconnect">重新连接</UButton>
        <UButton size="xs" variant="ghost" :disabled="loading || !!error" @click="toggleFollowing">{{ following ? '暂停滚动' : '继续滚动' }}</UButton>
      </template>
    </ViewerFooter>
    <UModal v-model:open="rulesOpen" title="日志关键词着色" description="所有日志共用，纯文本匹配且忽略大小写；列表靠前的规则优先。">
      <template #body>
        <div class="space-y-3">
          <div v-for="(rule, index) in rules" :key="index" class="flex gap-2">
            <UInput v-model="rule.keyword" placeholder="关键词" class="min-w-0 flex-1" />
            <USelect v-model="rule.color" :items="colorItems" class="w-20" />
            <UButton icon="i-lucide-trash-2" aria-label="删除关键词" color="neutral" variant="ghost" @click="rules.splice(index, 1)" />
          </div>
          <UButton variant="ghost" icon="i-lucide-plus" @click="rules.push({ keyword: '', color: 'red' })">添加关键词</UButton>
          <p v-if="rulesError" class="text-sm text-error">{{ rulesError }}</p>
        </div>
      </template>
      <template #footer><UButton :loading="rulesSaving" :disabled="rules.some(rule => !rule.keyword.trim())" @click="saveRules">保存规则</UButton></template>
    </UModal>
  </section>
</template>
