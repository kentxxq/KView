<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { desktopInvoke as invoke } from "../services/desktop-api";
import type { BinaryChunk, OpenedFile } from "../types/document";
import { decodeBase64 } from "../utils/binary";
import { errorMessage } from "../utils/error";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile }>();

const BYTES_PER_ROW = 16;
const CHUNK_BYTES = 64 * 1024;
const ROW_HEIGHT = 22;
const HEADER_HEIGHT = 32;
const OVERSCAN_ROWS = 12;
const MAX_TRACK_HEIGHT = 8_000_000;

const scrollElement = ref<HTMLElement>();
const scrollTop = ref(0);
const viewportHeight = ref(0);
const cacheRevision = ref(0);
const error = ref("");
const chunks = new Map<number, Uint8Array>();
const loadingChunks = new Set<number>();
let resizeObserver: ResizeObserver | undefined;

const rowCount = computed(() => Math.ceil(props.file.size / BYTES_PER_ROW));
const naturalTrackHeight = computed(() => rowCount.value * ROW_HEIGHT);
const normalizedScrolling = computed(() => naturalTrackHeight.value > MAX_TRACK_HEIGHT);
const trackHeight = computed(() =>
  Math.max(1, Math.min(naturalTrackHeight.value, MAX_TRACK_HEIGHT)),
);
const viewportRows = computed(() => Math.max(1, Math.ceil(viewportHeight.value / ROW_HEIGHT)));
const contentScrollTop = computed(() => Math.max(0, scrollTop.value - HEADER_HEIGHT));

const firstVisibleRow = computed(() => {
  if (!rowCount.value) return 0;
  if (!normalizedScrolling.value) {
    return Math.max(0, Math.floor(contentScrollTop.value / ROW_HEIGHT) - OVERSCAN_ROWS);
  }
  const scrollRange = Math.max(1, trackHeight.value - viewportHeight.value);
  const maxFirstRow = Math.max(0, rowCount.value - viewportRows.value);
  return Math.max(
    0,
    Math.floor((contentScrollTop.value / scrollRange) * maxFirstRow) - OVERSCAN_ROWS,
  );
});

const lastVisibleRow = computed(() =>
  Math.min(rowCount.value, firstVisibleRow.value + viewportRows.value + OVERSCAN_ROWS * 2),
);

const renderedTop = computed(() => {
  if (!normalizedScrolling.value) return firstVisibleRow.value * ROW_HEIGHT;
  const renderedHeight = (lastVisibleRow.value - firstVisibleRow.value) * ROW_HEIGHT;
  return Math.max(
    0,
    Math.min(
      contentScrollTop.value - OVERSCAN_ROWS * ROW_HEIGHT,
      trackHeight.value - renderedHeight,
    ),
  );
});

const offsetDigits = computed(() =>
  Math.max(8, Math.max(0, props.file.size - 1).toString(16).length),
);

function chunkOffsetFor(byteOffset: number) {
  return Math.floor(byteOffset / CHUNK_BYTES) * CHUNK_BYTES;
}

function byteAt(byteOffset: number) {
  void cacheRevision.value;
  const chunkOffset = chunkOffsetFor(byteOffset);
  return chunks.get(chunkOffset)?.[byteOffset - chunkOffset];
}

const rows = computed(() =>
  Array.from({ length: Math.max(0, lastVisibleRow.value - firstVisibleRow.value) }, (_, index) => {
    const rowIndex = firstVisibleRow.value + index;
    const offset = rowIndex * BYTES_PER_ROW;
    const bytes = Array.from(
      { length: Math.min(BYTES_PER_ROW, props.file.size - offset) },
      (_, byteIndex) => byteAt(offset + byteIndex),
    );
    return {
      rowIndex,
      offset,
      offsetLabel: offset.toString(16).toUpperCase().padStart(offsetDigits.value, "0"),
      bytes,
      ascii: bytes
        .map((byte) =>
          byte === undefined ? " " : byte >= 0x20 && byte <= 0x7e ? String.fromCharCode(byte) : ".",
        )
        .join(""),
    };
  }),
);

async function loadChunk(offset: number) {
  if (chunks.has(offset) || loadingChunks.has(offset) || error.value) return;
  loadingChunks.add(offset);
  try {
    const chunk = await invoke<BinaryChunk>("read_binary_chunk", {
      path: props.file.path,
      offset,
      length: CHUNK_BYTES,
      expectedModifiedAt: props.file.modifiedAt,
    });
    chunks.set(chunk.offset, decodeBase64(chunk.base64));
    cacheRevision.value += 1;
    pruneChunkCache();
  } catch (reason) {
    error.value = errorMessage(reason);
  } finally {
    loadingChunks.delete(offset);
  }
}

function pruneChunkCache() {
  if (!rowCount.value) return;
  const firstChunk = chunkOffsetFor(firstVisibleRow.value * BYTES_PER_ROW);
  const lastByte = Math.min(props.file.size - 1, lastVisibleRow.value * BYTES_PER_ROW - 1);
  const lastChunk = chunkOffsetFor(lastByte);
  const keepFrom = Math.max(0, firstChunk - CHUNK_BYTES);
  const keepThrough = lastChunk + CHUNK_BYTES;
  for (const offset of chunks.keys()) {
    if (offset < keepFrom || offset > keepThrough) chunks.delete(offset);
  }
}

async function loadVisibleChunks(firstRow: number, lastRow: number) {
  if (lastRow <= firstRow) return;
  const firstChunk = chunkOffsetFor(firstRow * BYTES_PER_ROW);
  const lastByte = Math.min(props.file.size - 1, lastRow * BYTES_PER_ROW - 1);
  const lastChunk = chunkOffsetFor(lastByte);
  const required: number[] = [];
  for (let offset = firstChunk; offset <= lastChunk; offset += CHUNK_BYTES) required.push(offset);
  await Promise.all(required.map(loadChunk));
}

function handleScroll() {
  scrollTop.value = scrollElement.value?.scrollTop ?? 0;
}

watch(
  [firstVisibleRow, lastVisibleRow],
  ([firstRow, lastRow]) => {
    void loadVisibleChunks(firstRow, lastRow);
  },
  { immediate: true },
);

onMounted(() => {
  if (!scrollElement.value) return;
  viewportHeight.value = scrollElement.value.clientHeight;
  resizeObserver = new ResizeObserver(([entry]) => {
    viewportHeight.value = entry.contentRect.height;
  });
  resizeObserver.observe(scrollElement.value);
});

onBeforeUnmount(() => resizeObserver?.disconnect());
</script>

<template>
  <section class="flex size-full min-h-0 flex-col bg-inverted text-white">
    <UAlert
      v-if="error"
      class="m-3 shrink-0"
      color="error"
      variant="subtle"
      icon="i-lucide-triangle-alert"
      title="无法继续读取文件"
      :description="error"
    />
    <div
      ref="scrollElement"
      class="min-h-0 flex-1 overflow-auto font-mono text-xs"
      @scroll="handleScroll"
    >
      <div
        class="sticky top-0 z-10 flex h-8 min-w-max items-center border-b border-white/10 bg-inverted px-4 text-white/45"
      >
        <span class="mr-6" :style="{ width: `${offsetDigits}ch` }">OFFSET</span>
        <span class="grid gap-x-2" style="grid-template-columns: repeat(16, 1rem)">
          <span v-for="index in 16" :key="index" class="w-4 text-center">{{
            (index - 1).toString(16).toUpperCase()
          }}</span>
        </span>
        <span class="ml-6" style="width: 16ch">ASCII</span>
      </div>
      <div class="relative min-w-max" :style="{ height: `${trackHeight}px` }">
        <div
          class="absolute left-0 top-0 px-4"
          :style="{ transform: `translateY(${renderedTop}px)` }"
        >
          <div
            v-for="row in rows"
            :key="row.rowIndex"
            class="flex items-center text-white/85"
            :style="{ height: `${ROW_HEIGHT}px` }"
          >
            <span
              class="mr-6 shrink-0 text-right text-sky-300/75"
              :style="{ width: `${offsetDigits}ch` }"
              >{{ row.offsetLabel }}</span
            >
            <span class="grid gap-x-2" style="grid-template-columns: repeat(16, 1rem)">
              <span v-for="index in 16" :key="index" class="w-4 text-center">
                {{ row.bytes[index - 1]?.toString(16).toUpperCase().padStart(2, "0") ?? "" }}
              </span>
            </span>
            <span class="ml-6 whitespace-pre text-emerald-200/80" style="width: 16ch">{{
              row.ascii
            }}</span>
          </div>
          <div v-if="!rowCount" class="py-8 text-white/45">空文件</div>
        </div>
      </div>
    </div>
    <ViewerFooter :file-size="file.size">
      <template #status>HEX · 每行 16 bytes · 只读</template>
    </ViewerFooter>
  </section>
</template>
