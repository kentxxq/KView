<script setup lang="ts">
import { computed, ref, useId, watch } from "vue";
import type { ChapterItem } from "../types/chapter";
import ChapterTreeNode from "./ChapterTreeNode.vue";

const props = defineProps<{
  items: ChapterItem[];
  activeId?: string;
  documentId: string;
  loading?: boolean;
}>();
const emit = defineEmits<{ select: [id: string] }>();
const desktopOpen = ref(true);
const mobileOpen = ref(false);
const toggleButton = ref<HTMLButtonElement>();
const panelId = useId();
const available = computed(() => !props.loading && props.items.length > 0);

watch(() => props.documentId, () => {
  desktopOpen.value = true;
  mobileOpen.value = false;
});
watch(available, (value) => {
  if (!value) mobileOpen.value = false;
});

function closeMobile() {
  mobileOpen.value = false;
  toggleButton.value?.focus();
}

function select(id: string) {
  emit('select', id);
  if (mobileOpen.value) closeMobile();
}
</script>

<template>
  <div class="relative flex min-h-0 min-w-0 flex-1 flex-col" @keydown.esc="closeMobile">
    <div v-if="available" class="flex shrink-0 border-b border-muted bg-default px-2 py-1">
      <button type="button" class="hidden items-center gap-1.5 rounded px-2 py-1 text-xs text-muted hover:bg-elevated lg:flex"
        :aria-expanded="desktopOpen" :aria-controls="panelId" @click="desktopOpen = !desktopOpen">
        <UIcon name="i-lucide-list-tree" class="size-4" />{{ desktopOpen ? "收起目录" : "展开目录" }}
      </button>
      <button ref="toggleButton" type="button" class="flex items-center gap-1.5 rounded px-2 py-1 text-xs text-muted hover:bg-elevated lg:hidden"
        :aria-expanded="mobileOpen" :aria-controls="panelId" @click="mobileOpen = !mobileOpen">
        <UIcon name="i-lucide-list-tree" class="size-4" />章节目录
      </button>
    </div>
    <div class="relative flex min-h-0 flex-1">
      <button v-if="available && mobileOpen" type="button" aria-label="关闭章节目录"
        class="absolute inset-0 z-10 bg-black/30 lg:hidden" @click="closeMobile" />
      <nav v-if="available" :id="panelId" aria-label="章节目录"
        class="absolute inset-y-0 left-0 z-20 w-64 shrink-0 overflow-auto border-r border-muted bg-default p-3 lg:static lg:z-auto"
        :class="[mobileOpen ? 'block' : 'hidden', desktopOpen ? 'lg:block' : 'lg:hidden']">
        <p class="mb-2 px-2 text-xs font-semibold text-muted">章节目录</p>
        <ul :key="documentId">
          <ChapterTreeNode v-for="item in items" :key="item.id" :item="item" :active-id="activeId" @select="select" />
        </ul>
      </nav>
      <div class="flex min-h-0 min-w-0 flex-1 flex-col"><slot /></div>
    </div>
  </div>
</template>
