<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import * as XLSX from "xlsx";
import type { OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile; content: string; binaryContent?: Uint8Array<ArrayBuffer>; readOnly: boolean }>();
const workbook = ref<XLSX.WorkBook>();
const sheetName = ref("");
const rows = ref<unknown[][]>([]);
const query = ref("");
const selected = ref<{ address: string; value: unknown }>();
const scrollTop = ref(0);
const viewportHeight = ref(500);
const error = ref("");
const rowHeight = 34;
const overscan = 8;

const columnCount = computed(() => Math.min(100, Math.max(1, ...rows.value.map((row) => row.length))));
const filteredRows = computed(() => {
  const term = query.value.trim().toLocaleLowerCase();
  if (!term) return rows.value.map((row, index) => ({ row, index }));
  return rows.value.map((row, index) => ({ row, index })).filter(({ row }) => row.some((cell) => String(cell ?? "").toLocaleLowerCase().includes(term)));
});
const start = computed(() => Math.max(0, Math.floor(scrollTop.value / rowHeight) - overscan));
const visibleCount = computed(() => Math.ceil(viewportHeight.value / rowHeight) + overscan * 2);
const visibleRows = computed(() => filteredRows.value.slice(start.value, start.value + visibleCount.value));

function selectSheet(name: string) {
  if (!workbook.value) return;
  sheetName.value = name;
  rows.value = XLSX.utils.sheet_to_json<unknown[]>(workbook.value.Sheets[name], { header: 1, defval: "", raw: false });
  scrollTop.value = 0;
  selected.value = undefined;
}

function columnLabel(index: number) {
  return XLSX.utils.encode_col(index);
}

function onScroll(event: Event) {
  const target = event.currentTarget as HTMLElement;
  scrollTop.value = target.scrollTop;
  viewportHeight.value = target.clientHeight;
}

function handleSheetChange(value: unknown) {
  if (typeof value === "string") selectSheet(value);
}

function loadWorkbook() {
  workbook.value = undefined;
  error.value = "";
  try {
    if (!props.binaryContent) throw new Error("表格二进制内容不可用");
    workbook.value = XLSX.read(props.binaryContent, { type: "array", cellFormula: true });
    if (!workbook.value.SheetNames.length) throw new Error("工作簿中没有工作表");
    selectSheet(workbook.value.SheetNames[0]);
  } catch (cause) {
    error.value = `无法解析 ${props.file.extension.toUpperCase()}。文件可能已损坏、加密或并非有效工作簿。${cause instanceof Error ? ` ${cause.message}` : ""}`;
  }
}

onMounted(loadWorkbook);
watch(() => [props.file.path, props.file.modifiedAt], loadWorkbook);
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-default">
    <div v-if="error" class="grid flex-1 place-items-center p-8 text-center text-sm text-error">{{ error }}</div>
    <template v-else>
      <div class="grid shrink-0 border-b border-muted bg-muted/70 text-xs font-semibold text-muted" :style="{ gridTemplateColumns: `48px repeat(${columnCount}, minmax(140px, 1fr))`, minWidth: `${48 + columnCount * 140}px` }">
        <div class="sticky left-0 z-20 border-r border-muted bg-muted px-2 py-2">#</div>
        <div v-for="column in columnCount" :key="column" class="border-r border-muted px-2 py-2">{{ columnLabel(column - 1) }}</div>
      </div>
      <div class="min-h-0 flex-1 overflow-auto" @scroll="onScroll">
        <div class="relative" :style="{ height: `${filteredRows.length * rowHeight}px`, minWidth: `${48 + columnCount * 140}px` }">
          <div v-for="item in visibleRows" :key="item.index" class="absolute left-0 grid w-full border-b border-muted text-sm" :style="{ top: `${filteredRows.indexOf(item) * rowHeight}px`, height: `${rowHeight}px`, gridTemplateColumns: `48px repeat(${columnCount}, minmax(140px, 1fr))` }">
            <div class="sticky left-0 z-10 border-r border-muted bg-muted/90 px-2 py-2 text-xs text-muted">{{ item.index + 1 }}</div>
            <button v-for="column in columnCount" :key="column" class="truncate border-r border-muted px-2 py-1.5 text-left hover:bg-primary/10" @click="selected = { address: `${columnLabel(column - 1)}${item.index + 1}`, value: item.row[column - 1] }">{{ item.row[column - 1] }}</button>
          </div>
        </div>
      </div>
      <div class="flex h-9 shrink-0 items-center border-t border-muted px-3 text-xs text-muted">
        <span v-if="selected"><strong class="text-highlighted">{{ selected.address }}</strong> · {{ selected.value || "空单元格" }}</span>
        <span v-else>选择单元格以查看地址和值</span>
      </div>
    </template>
    <ViewerFooter :file-size="file.size" notice="公式不会重新计算，宏、图表、复杂样式和 Office 专有排版可能与原工作簿不同；KView 不会执行文档链接、宏或脚本。">
      <template #status>
        {{ file.extension.toUpperCase() }}<template v-if="sheetName"> · {{ sheetName }} · {{ filteredRows.length }} 行</template> · 只读
      </template>
      <template v-if="!error" #actions>
        <USelect :model-value="sheetName" class="w-36" size="xs" :items="workbook?.SheetNames ?? []" aria-label="切换工作表" @update:model-value="handleSheetChange" />
        <UPopover :content="{ side: 'top', align: 'end', sideOffset: 8 }" :ui="{ content: 'compatibility-popover' }">
          <UButton class="viewer-footer-button" color="neutral" variant="ghost" size="xs" icon="i-lucide-search" label="搜索" />
          <template #content>
            <div class="w-72 space-y-2">
              <UInput v-model="query" icon="i-lucide-search" size="sm" placeholder="搜索当前 Sheet" autofocus />
              <p class="text-xs text-muted">当前显示 {{ filteredRows.length }} 行</p>
            </div>
          </template>
        </UPopover>
      </template>
    </ViewerFooter>
  </div>
</template>
