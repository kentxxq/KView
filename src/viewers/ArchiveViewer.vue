<script setup lang="ts">
import { computed, ref } from "vue";
import type { ArchiveDocument, OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{
  file: OpenedFile;
  content?: string;
  structuredContent?: ArchiveDocument;
  readOnly: boolean;
}>();
const query = ref("");

const archive = computed<ArchiveDocument | null>(() => {
  if (props.structuredContent) return props.structuredContent;
  if (!props.content) return null;
  try {
    return JSON.parse(props.content) as ArchiveDocument;
  } catch {
    return null;
  }
});

const filteredEntries = computed(() => {
  if (!archive.value) return [];
  const term = query.value.trim().toLocaleLowerCase();
  if (!term) return archive.value.entries;
  return archive.value.entries.filter((entry) => entry.path.toLocaleLowerCase().includes(term));
});

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GiB`;
}
</script>

<template>
  <div v-if="!archive" class="grid size-full place-items-center bg-default p-6 text-sm text-error">
    压缩包结构化内容缺失或格式无效。
  </div>
  <div v-else class="flex size-full min-h-0 flex-col bg-default">
    <div
      class="grid shrink-0 grid-cols-[minmax(0,1fr)_8rem_8rem] border-b border-muted bg-muted/70 px-4 py-2 text-xs font-semibold text-muted"
    >
      <span>名称</span>
      <span class="text-right">原始大小</span>
      <span class="text-right">压缩后</span>
    </div>
    <div class="min-h-0 flex-1 overflow-auto">
      <div
        v-for="(entry, index) in filteredEntries"
        :key="`${index}-${entry.path}`"
        class="grid grid-cols-[minmax(0,1fr)_8rem_8rem] items-center border-b border-muted/60 px-4 py-2 text-xs hover:bg-elevated"
      >
        <span
          class="truncate font-mono"
          :class="entry.isDirectory ? 'font-semibold text-primary' : 'text-default'"
          >{{ entry.path }}</span
        >
        <span class="text-right tabular-nums text-muted">{{
          entry.isDirectory ? "—" : formatBytes(entry.size)
        }}</span>
        <span class="text-right tabular-nums text-muted">{{
          entry.isDirectory || entry.compressedSize === null
            ? "—"
            : formatBytes(entry.compressedSize)
        }}</span>
      </div>
      <div v-if="!filteredEntries.length" class="grid h-40 place-items-center text-sm text-muted">
        没有匹配的压缩包条目
      </div>
    </div>
    <ViewerFooter
      :file-size="file.size"
      :notice="archive.truncated ? '压缩包条目较多，仅显示前 5000 项。' : undefined"
    >
      <template #status>
        {{ archive.format }} · {{ archive.entries.length }} 个条目 · 只读
      </template>
      <template #actions>
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
            <div class="w-72 space-y-2">
              <UInput
                v-model="query"
                icon="i-lucide-search"
                size="sm"
                placeholder="搜索文件或目录"
                autofocus
              />
              <p class="text-xs text-muted">当前显示 {{ filteredEntries.length }} 个条目</p>
            </div>
          </template>
        </UPopover>
      </template>
    </ViewerFooter>
  </div>
</template>
