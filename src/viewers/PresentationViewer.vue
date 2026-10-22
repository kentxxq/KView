<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import type { OpenedFile, PresentationDocument } from "../types/document";
import ChapterLayout from "../components/ChapterLayout.vue";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{
  file: OpenedFile;
  structuredContent: PresentationDocument;
}>();
const preview = ref<HTMLElement>();
const activeIndex = ref(0);
const slides = computed(() => props.structuredContent.slides);
const documentId = computed(() => JSON.stringify([props.file.path, props.file.modifiedAt]));
const chapters = computed(() => slides.value.map((slide, index) => ({
  id: `kview-slide-${index}`,
  label: `${slide.index} · ${slide.title || '未命名幻灯片'}`,
})));
const activeId = computed(() => `kview-slide-${activeIndex.value}`);

function updateActiveSlide() {
  const container = preview.value;
  if (!container) return;
  if (container.scrollHeight > container.clientHeight && container.scrollTop + container.clientHeight >= container.scrollHeight - 1) {
    activeIndex.value = slides.value.length - 1;
    return;
  }
  const top = container.getBoundingClientRect().top + Math.min(container.clientHeight / 4, 160);
  let current = 0;
  for (const [index, slide] of container.querySelectorAll<HTMLElement>("[data-slide-id]").entries()) {
    if (slide.getBoundingClientRect().top > top) break;
    current = index;
  }
  activeIndex.value = current;
}

function selectSlide(id: string) {
  const container = preview.value;
  const slide = container?.querySelector<HTMLElement>(`[data-slide-id="${id}"]`);
  if (!container || !slide) return;
  container.scrollTo({ top: container.scrollTop + slide.getBoundingClientRect().top - container.getBoundingClientRect().top - 16 });
  activeIndex.value = chapters.value.findIndex((chapter) => chapter.id === id);
}

watch(() => props.structuredContent, async () => {
  activeIndex.value = 0;
  await nextTick();
  preview.value?.scrollTo({ top: 0 });
}, { flush: "post" });
</script>

<template>
  <section class="flex size-full min-h-0 flex-col bg-default">
    <ChapterLayout :items="chapters" :active-id="activeId" :document-id="documentId" @select="selectSlide">
      <div ref="preview" class="min-h-0 flex-1 overflow-auto bg-muted/30 p-4"
        @scroll.passive="updateActiveSlide" @load.capture="updateActiveSlide">
        <div class="mx-auto flex max-w-5xl flex-col gap-6">
          <section v-for="(slide, index) in slides" :key="slide.index" :data-slide-id="chapters[index]!.id"
            :aria-label="`第 ${slide.index} 张幻灯片`" class="min-w-0 rounded border border-muted bg-default shadow-sm">
            <header class="border-b border-muted px-6 py-2 text-xs text-muted">第 {{ slide.index }} 页</header>
            <article v-if="slide.html" class="presentation-content aspect-video p-6 text-default" v-html="slide.html" />
            <div v-else class="grid aspect-video place-items-center text-sm text-muted">本页没有可显示的内容</div>
          </section>
        </div>
      </div>
    </ChapterLayout>
    <ViewerFooter :file-size="file.size" notice="内容预览使用系统字体；不还原原始坐标排版、动画和复杂图表。">
      <template #status>{{ file.extension.toUpperCase() }} · {{ activeIndex + 1 }}/{{ slides.length }} 页 · 只读</template>
    </ViewerFooter>
  </section>
</template>

<style scoped>
.presentation-content { overflow-wrap: anywhere; }
.presentation-content :deep(p) { margin: 0.65em 0; }
.presentation-content :deep(h1), .presentation-content :deep(h2), .presentation-content :deep(h3) { margin: 0.8em 0 0.5em; font-weight: 600; }
.presentation-content :deep(h1) { font-size: 1.8em; }
.presentation-content :deep(h2) { font-size: 1.5em; }
.presentation-content :deep(h3) { font-size: 1.2em; }
.presentation-content :deep(img) { max-width: 100%; height: auto; }
.presentation-content :deep(table) { border-collapse: collapse; max-width: 100%; }
.presentation-content :deep(td), .presentation-content :deep(th) { border: 1px solid var(--ui-border); padding: 0.4em 0.65em; }
.presentation-content :deep(ul) { list-style: disc; padding-left: 1.5em; }
.presentation-content :deep(ol) { list-style: decimal; padding-left: 1.5em; }
</style>
