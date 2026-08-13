<script setup lang="ts">
import { ref } from "vue";

defineOptions({ name: "EbookTocNode" });

interface EbookTocItem {
  label: string;
  href: string;
  children?: EbookTocItem[];
}

const props = withDefaults(
  defineProps<{
    item: EbookTocItem;
    depth?: number;
  }>(),
  {
    depth: 0,
  },
);

const emit = defineEmits<{ navigate: [href: string] }>();
const expanded = ref(props.depth < 1);
</script>

<template>
  <div>
    <div
      class="flex min-w-0 items-center rounded hover:bg-elevated"
      :style="{ paddingLeft: `${depth * 12}px` }"
    >
      <button
        v-if="item.children?.length"
        type="button"
        class="grid size-7 shrink-0 place-items-center text-muted"
        :aria-label="expanded ? '收起目录' : '展开目录'"
        @click="expanded = !expanded"
      >
        <UIcon
          name="i-lucide-chevron-right"
          class="size-3.5 transition-transform"
          :class="expanded && 'rotate-90'"
        />
      </button>
      <span v-else class="inline-block size-7 shrink-0" />
      <button
        type="button"
        class="min-w-0 flex-1 truncate py-1.5 pr-2 text-left text-xs"
        :title="item.label"
        @click="emit('navigate', item.href)"
      >
        {{ item.label || "未命名章节" }}
      </button>
    </div>
    <div v-if="expanded && item.children?.length">
      <EbookTocNode
        v-for="(child, index) in item.children"
        :key="`${child.href}-${index}`"
        :item="child"
        :depth="depth + 1"
        @navigate="emit('navigate', $event)"
      />
    </div>
  </div>
</template>
