<script setup lang="ts">
import { ref } from "vue";
import type { ChapterItem } from "../types/chapter";

defineOptions({ name: "ChapterTreeNode" });
withDefaults(defineProps<{ item: ChapterItem; activeId?: string; depth?: number }>(), {
  depth: 0,
});
const emit = defineEmits<{ select: [id: string] }>();
const expanded = ref(true);
</script>

<template>
  <li>
    <div class="flex min-w-0 items-center rounded hover:bg-elevated"
      :class="activeId === item.id && 'bg-primary/10 text-primary'"
      :style="{ paddingLeft: `${depth * 12}px` }">
      <button v-if="item.children?.length" type="button"
        class="grid size-7 shrink-0 place-items-center rounded text-muted focus-visible:outline-2"
        :aria-label="`${expanded ? '收起' : '展开'}${item.label || '未命名章节'}`"
        :aria-expanded="expanded" @click="expanded = !expanded">
        <UIcon name="i-lucide-chevron-right" class="size-3.5" :class="expanded && 'rotate-90'" />
      </button>
      <span v-else class="size-7 shrink-0" />
      <button type="button" class="min-w-0 flex-1 truncate rounded py-1.5 pr-2 text-left text-xs focus-visible:outline-2"
        :title="item.label || '未命名章节'" :aria-current="activeId === item.id ? 'location' : undefined"
        @click="emit('select', item.id)">
        {{ item.label || "未命名章节" }}
      </button>
    </div>
    <ul v-if="item.children?.length" v-show="expanded">
      <ChapterTreeNode v-for="child in item.children" :key="child.id" :item="child"
        :active-id="activeId" :depth="depth + 1" @select="emit('select', $event)" />
    </ul>
  </li>
</template>
