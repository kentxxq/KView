<script setup lang="ts">
import { libraryState } from "../state/library";

defineProps<{ open: boolean }>();
const emit = defineEmits<{
  "update:open": [value: boolean];
  open: [path: string];
  favorite: [path: string, name: string];
  removeRecent: [path: string];
}>();

function openPath(path: string) {
  emit("open", path);
  emit("update:open", false);
}
</script>

<template>
  <UModal :open="open" title="文件库" description="最近打开与收藏仅保存本地路径和时间" @update:open="$emit('update:open', $event)">
    <template #body>
      <div class="space-y-6">
        <section>
          <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-muted">收藏</h3>
          <div v-if="libraryState.favorites.length" class="space-y-1">
            <div v-for="entry in libraryState.favorites" :key="entry.path" class="group flex items-center gap-2 rounded-lg p-2 hover:bg-elevated">
              <UButton class="min-w-0 flex-1 justify-start" color="neutral" variant="ghost" icon="i-lucide-star" :label="entry.name" :title="entry.path" @click="openPath(entry.path)" />
              <UButton color="neutral" variant="ghost" icon="i-lucide-star-off" aria-label="取消收藏" @click="$emit('favorite', entry.path, entry.name)" />
            </div>
          </div>
          <p v-else class="rounded-lg border border-dashed border-muted p-4 text-center text-sm text-muted">还没有收藏文件。</p>
        </section>
        <section>
          <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-muted">最近打开</h3>
          <div v-if="libraryState.recent.length" class="max-h-72 space-y-1 overflow-auto">
            <div v-for="entry in libraryState.recent" :key="entry.path" class="group flex items-center gap-2 rounded-lg p-2 hover:bg-elevated">
              <UButton class="min-w-0 flex-1 justify-start" color="neutral" variant="ghost" icon="i-lucide-clock-3" :label="entry.name" :title="entry.path" @click="openPath(entry.path)" />
              <UButton color="neutral" variant="ghost" icon="i-lucide-x" aria-label="从最近打开中移除" @click="$emit('removeRecent', entry.path)" />
            </div>
          </div>
          <p v-else class="rounded-lg border border-dashed border-muted p-4 text-center text-sm text-muted">还没有最近打开记录。</p>
        </section>
      </div>
    </template>
  </UModal>
</template>

