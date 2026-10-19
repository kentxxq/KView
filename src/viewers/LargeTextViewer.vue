<script setup lang="ts">
import {
  computed,
  nextTick,
  onActivated,
  onBeforeUnmount,
  onDeactivated,
  onMounted,
  reactive,
  ref,
} from "vue";
import { desktopInvoke as invoke } from "../services/desktop-api";
import type {
  LargeTextIndexStatus,
  LargeTextLinesResult,
  LargeTextSessionInfo,
  OpenedFile,
} from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";

const ROW_HEIGHT = 21;
const OVERSCAN_ROWS = 24;
const REQUEST_LINES = 2048;
const INDEX_POLL_MS = 500;

const props = defineProps<{
  file: OpenedFile;
  session: LargeTextSessionInfo;
}>();

const initialEndLine =
  props.session.initialLines[props.session.initialLines.length - 1]?.lineNumber ?? 1;
const indexStatus = reactive<LargeTextIndexStatus>({
  indexedBytes: props.session.indexedBytes,
  fileSize: props.session.fileSize,
  indexedLineCount: Math.max(props.session.indexedLineCount, initialEndLine),
  complete: false,
  totalLines: null,
  error: null,
});
const batches = ref<LargeTextLinesResult[]>([
  {
    startLine: props.session.initialLines[0]?.lineNumber ?? 1,
    endLine: initialEndLine,
    lines: props.session.initialLines,
  },
]);
const lineCache = reactive(new Map<number, string>());
const pendingStarts = new Map<number, number>();
const loading = ref(false);
const error = ref("");
const visibleStartLine = ref(1);
const visibleEndLine = ref(1);
let pollTimer: number | undefined;
let loadGeneration = 0;
let scheduledStartLine = 0;
let scheduledEndLine = 0;
let disposed = false;

for (const line of props.session.initialLines) lineCache.set(line.lineNumber, line.content);

const virtualLineCount = computed(() =>
  Math.max(
    1,
    indexStatus.complete && indexStatus.totalLines !== null
      ? indexStatus.totalLines
      : indexStatus.indexedLineCount,
  ),
);
const virtualItems = computed(() => new Array(virtualLineCount.value));
const reservedLineNumberDigits = Math.max(8, String(Math.floor(props.session.fileSize) + 1).length);
const lineNumberWidth = `calc(${reservedLineNumberDigits}ch + 1.5rem + 1px)`;
const progress = computed(() =>
  indexStatus.fileSize > 0
    ? Math.min(100, (indexStatus.indexedBytes / indexStatus.fileSize) * 100)
    : 100,
);
const notice = computed(() => {
  const total = indexStatus.totalLines === null ? "索引中" : `${indexStatus.totalLines} 行`;
  return [
    `大文件只读模式`,
    `索引 ${progress.value.toFixed(1)}%（${formatBytes(indexStatus.indexedBytes)} / ${formatBytes(indexStatus.fileSize)}）`,
    `当前第 ${visibleStartLine.value}–${visibleEndLine.value} 行`,
    `总行数：${total}`,
  ];
});

const virtualizeOptions = {
  estimateSize: ROW_HEIGHT,
  overscan: OVERSCAN_ROWS,
  skipMeasurement: true,
  getItemKey: (index: number) => index + 1,
  onChange: (
    virtualizer: { getVirtualItems: () => Array<{ index: number }> },
    scrolling: boolean,
  ) => {
    const items = virtualizer.getVirtualItems();
    if (!items.length) return;
    const start = items[0].index + 1;
    const end = items[items.length - 1].index + 1;
    visibleStartLine.value = start;
    visibleEndLine.value = end;
    const missing = hasMissingLine(start, end);
    if (scrolling) {
      scheduledStartLine = start;
      scheduledEndLine = end;
      loadGeneration += 1;
      loading.value = missing;
      return;
    }
    if (!missing) {
      loading.value = false;
      return;
    }
    if (
      start === scheduledStartLine &&
      end === scheduledEndLine &&
      [...pendingStarts.values()].some((generation) => generation === loadGeneration)
    ) {
      return;
    }
    scheduledStartLine = start;
    scheduledEndLine = end;
    scheduleVisibleLoad(start, end);
  },
};

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MiB`;
}

function rebuildLineCache() {
  lineCache.clear();
  for (const batch of batches.value) {
    for (const line of batch.lines) lineCache.set(line.lineNumber, line.content);
  }
}

function rememberBatch(batch: LargeTextLinesResult, focusLine: number) {
  const remaining = batches.value.filter(
    (item) => item.endLine < batch.startLine || item.startLine > batch.endLine,
  );
  remaining.push(batch);
  remaining.sort((left, right) => {
    const leftCenter = (left.startLine + left.endLine) / 2;
    const rightCenter = (right.startLine + right.endLine) / 2;
    return Math.abs(leftCenter - focusLine) - Math.abs(rightCenter - focusLine);
  });
  batches.value = remaining.slice(0, 3).sort((left, right) => left.startLine - right.startLine);
  rebuildLineCache();
}

function batchContaining(lineNumber: number) {
  return batches.value.find(
    (batch) => lineNumber >= batch.startLine && lineNumber <= batch.endLine,
  );
}

async function loadBatch(
  startLine: number,
  focusLine: number,
  generation: number,
  prefetch = false,
) {
  if (generation !== loadGeneration) return;
  const safeStart = Math.max(1, Math.min(startLine, virtualLineCount.value));
  if (pendingStarts.get(safeStart) === generation || batchContaining(safeStart)) return;
  pendingStarts.set(safeStart, generation);
  if (!prefetch) loading.value = true;
  try {
    const result = await invoke<LargeTextLinesResult>("read_text_lines", {
      sessionId: props.session.sessionId,
      startLine: safeStart,
      lineCount: REQUEST_LINES,
    });
    if (disposed || generation !== loadGeneration || !result.lines.length) return;
    rememberBatch(result, focusLine);
    if (!prefetch) {
      const previousStart = Math.max(1, result.startLine - REQUEST_LINES);
      const nextStart = result.endLine + 1;
      if (previousStart < result.startLine) {
        void loadBatch(previousStart, focusLine, generation, true);
      }
      if (nextStart <= virtualLineCount.value) {
        void loadBatch(nextStart, focusLine, generation, true);
      }
    }
  } catch (cause) {
    if (!disposed && generation === loadGeneration) {
      error.value = cause instanceof Error ? cause.message : String(cause);
    }
  } finally {
    if (pendingStarts.get(safeStart) === generation) pendingStarts.delete(safeStart);
    if (!prefetch && generation === loadGeneration) loading.value = false;
  }
}

function scheduleVisibleLoad(startLine: number, endLine: number) {
  loadGeneration += 1;
  const generation = loadGeneration;
  loading.value = hasMissingLine(startLine, endLine);
  void ensureVisibleLines(startLine, endLine, generation);
}

function hasMissingLine(startLine: number, endLine: number) {
  for (let lineNumber = startLine; lineNumber <= endLine; lineNumber++) {
    if (!lineCache.has(lineNumber)) return true;
  }
  return false;
}

async function ensureVisibleLines(startLine: number, endLine: number, generation: number) {
  if (error.value || generation !== loadGeneration) return;
  let missing: number | undefined;
  for (let lineNumber = startLine; lineNumber <= endLine; lineNumber++) {
    if (!lineCache.has(lineNumber)) {
      missing = lineNumber;
      break;
    }
  }
  if (missing === undefined) {
    loading.value = false;
    return;
  }
  await loadBatch(missing, Math.floor((startLine + endLine) / 2), generation);
}

async function pollIndexStatus() {
  if (disposed || indexStatus.complete || error.value) return;
  try {
    const status = await invoke<LargeTextIndexStatus>("get_large_text_index_status", {
      sessionId: props.session.sessionId,
    });
    Object.assign(indexStatus, status, {
      indexedLineCount: status.complete
        ? (status.totalLines ?? status.indexedLineCount)
        : Math.max(status.indexedLineCount, initialEndLine),
    });
    if (status.error) {
      error.value = status.error;
      stopPolling();
      return;
    }
    if (status.complete) {
      stopPolling();
      await nextTick();
      scheduleVisibleLoad(visibleStartLine.value, visibleEndLine.value);
    }
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    stopPolling();
  }
}

function startPolling() {
  if (pollTimer !== undefined || indexStatus.complete || error.value) return;
  void pollIndexStatus();
  pollTimer = window.setInterval(() => void pollIndexStatus(), INDEX_POLL_MS);
}

function stopPolling() {
  if (pollTimer === undefined) return;
  window.clearInterval(pollTimer);
  pollTimer = undefined;
}

onMounted(startPolling);
onActivated(startPolling);
onDeactivated(stopPolling);
onBeforeUnmount(() => {
  disposed = true;
  stopPolling();
  loadGeneration += 1;
  pendingStarts.clear();
  batches.value = [];
  lineCache.clear();
});
</script>

<template>
  <section class="relative flex size-full min-h-0 flex-col overflow-hidden bg-inverted text-white">
    <UAlert
      v-if="error"
      class="m-3 shrink-0"
      color="error"
      variant="subtle"
      icon="i-lucide-triangle-alert"
      title="无法继续读取文件"
      :description="error"
    />
    <UScrollArea
      class="min-h-0 min-w-0 flex-1 overflow-auto bg-inverted font-mono text-[13px]"
      orientation="vertical"
      :items="virtualItems"
      :virtualize="virtualizeOptions"
      :ui="{ viewport: 'min-w-max py-3', item: 'min-w-max' }"
      :style="{ overflow: 'scroll', overflowAnchor: 'none', scrollbarGutter: 'stable' }"
    >
      <template #default="{ index }">
        <div
          class="flex min-w-max whitespace-pre leading-[21px] text-white/90"
          :style="{ height: `${ROW_HEIGHT}px` }"
        >
          <span
            class="sticky left-0 z-10 shrink-0 select-none border-r border-white/10 bg-inverted px-3 text-right text-white/35"
            :style="{ width: lineNumberWidth, minWidth: lineNumberWidth }"
            >{{ index + 1 }}</span
          >
          <span class="min-w-max px-3">{{ lineCache.get(index + 1) ?? "" }}</span>
        </div>
      </template>
    </UScrollArea>
    <div
      v-if="loading"
      class="pointer-events-none absolute right-4 top-3 rounded bg-black/65 px-2 py-1 text-xs text-white/70"
    >
      正在读取…
    </div>
    <ViewerFooter :file-size="file.size" :notice="notice">
      <template #status>
        {{ (file.extension || "TEXT").toUpperCase() }}
        <template v-if="file.encoding"> · {{ file.encoding.toUpperCase() }}</template>
        · 大文件只读
      </template>
    </ViewerFooter>
  </section>
</template>
