<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { BinaryStructureDocument, OpenedFile } from "../types/document";
import { readBinaryStructure } from "../services/binary-structure";
import ViewerFooter from "../components/ViewerFooter.vue";
import HexViewer from "./HexViewer.vue";
import { errorMessage } from "../utils/error";

const props = defineProps<{ file: OpenedFile }>();
const mode = ref<"structure" | "hex">("structure");
const document = ref<BinaryStructureDocument>();
const error = ref("");
const loading = ref(true);
const tableKey = ref<"sections" | "imports" | "exports" | "symbols" | "programHeaders" | "dependencies">("sections");
const tableOptions = computed(() => [
  { value: "sections", label: "节区" },
  { value: "imports", label: "导入" },
  { value: "exports", label: "导出" },
  { value: "symbols", label: "符号" },
  ...(document.value?.format === "ELF" ? [{ value: "programHeaders", label: "程序头" }] : []),
  { value: "dependencies", label: "动态依赖" },
]);
const table = computed(() => document.value?.[tableKey.value]);
let generation = 0;

watch(() => [props.file.path, props.file.modifiedAt], async () => {
  const current = ++generation;
  loading.value = true;
  error.value = "";
  document.value = undefined;
  tableKey.value = "sections";
  mode.value = "structure";
  try {
    const result = await readBinaryStructure(props.file);
    if (current === generation) document.value = result;
  } catch (cause) {
    if (current === generation) error.value = errorMessage(cause);
  } finally {
    if (current === generation) loading.value = false;
  }
}, { immediate: true });

onBeforeUnmount(() => { generation++; });
</script>

<template>
  <section class="flex size-full min-h-0 flex-col bg-default">
    <div class="flex shrink-0 items-center gap-2 border-b border-muted px-4 py-2">
      <UButton :variant="mode === 'structure' ? 'soft' : 'ghost'" color="neutral" size="sm" label="结构" @click="mode = 'structure'" />
      <UButton :variant="mode === 'hex' ? 'soft' : 'ghost'" color="neutral" size="sm" label="十六进制" @click="mode = 'hex'" />
    </div>
    <HexViewer v-if="mode === 'hex'" class="min-h-0 flex-1" :file="file" />
    <template v-else>
      <div v-if="loading" class="grid min-h-0 flex-1 place-items-center text-sm text-muted">正在解析二进制结构…</div>
      <div v-else-if="error" class="grid min-h-0 flex-1 place-items-center p-6">
        <UAlert color="error" variant="subtle" title="无法解析二进制结构" :description="error" />
      </div>
      <template v-else-if="document">
        <dl class="grid shrink-0 grid-cols-2 gap-x-6 gap-y-2 border-b border-muted p-4 text-sm sm:grid-cols-3">
          <div><dt class="text-xs text-muted">格式 / 位数</dt><dd>{{ document.format }} · {{ document.bitness }} 位</dd></div>
          <div><dt class="text-xs text-muted">架构 / 字节序</dt><dd>{{ document.architecture }} · {{ document.byteOrder }}</dd></div>
          <div><dt class="text-xs text-muted">入口（PE 为 RVA）</dt><dd class="font-mono">{{ document.entryPoint }}</dd></div>
          <div v-if="document.imageBase"><dt class="text-xs text-muted">映像基址</dt><dd class="font-mono">{{ document.imageBase }}</dd></div>
          <div class="col-span-2"><dt class="text-xs text-muted">文件头</dt><dd class="break-all">{{ document.fileType }}</dd></div>
        </dl>
        <label class="flex shrink-0 items-center gap-3 px-4 py-2 text-sm">
          <span>结构表</span>
          <select v-model="tableKey" aria-label="选择二进制结构表" class="rounded border border-muted bg-default px-2 py-1">
            <option v-for="option in tableOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
          </select>
          <span class="text-xs text-muted">{{ table?.items.length }} 项</span>
        </label>
        <div :key="tableKey" class="min-h-0 flex-1 overflow-auto">
          <table class="w-full border-collapse text-left text-xs">
            <thead class="sticky top-0 bg-muted"><tr>
              <th class="px-4 py-2">名称</th><th class="px-4 py-2">地址 / 值</th><th class="px-4 py-2">文件偏移</th><th class="px-4 py-2">长度</th><th class="px-4 py-2">说明</th>
            </tr></thead>
            <tbody>
              <tr v-for="(row, index) in table?.items" :key="index" class="border-b border-muted/60 hover:bg-elevated">
                <td class="max-w-sm break-all px-4 py-2 font-mono">{{ row.name || '（无名称）' }}</td>
                <td class="whitespace-nowrap px-4 py-2 font-mono">{{ row.address ?? '—' }}</td>
                <td class="whitespace-nowrap px-4 py-2 font-mono">{{ row.offset ?? '—' }}</td>
                <td class="whitespace-nowrap px-4 py-2 font-mono">{{ row.size ?? '—' }}</td>
                <td class="max-w-lg break-words px-4 py-2 text-muted">{{ row.details }}</td>
              </tr>
            </tbody>
          </table>
          <p v-if="!table?.items.length" class="p-8 text-center text-sm text-muted">文件没有此类结构条目</p>
        </div>
      </template>
      <ViewerFooter :file-size="file.size" :notice="table?.truncated ? '结构条目较多，此表仅显示前 5000 项。' : undefined">
        <template #status>二进制结构 · 只读</template>
      </ViewerFooter>
    </template>
  </section>
</template>
