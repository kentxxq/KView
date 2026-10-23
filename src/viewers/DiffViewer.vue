<script setup lang="ts">
import { nextTick, onActivated, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { editor } from "monaco-editor";
import type { DocumentState } from "../types/document";
import { textModelFor } from "../services/text-models";
import { loadMonaco } from "../services/monaco-runtime";
import { errorMessage } from "../utils/error";

const props = defineProps<{ left: DocumentState; right: DocumentState; locked: boolean }>();
const emit = defineEmits<{ save: [side: "left" | "right"]; focus: [side: "left" | "right"] }>();
const container = ref<HTMLElement>();
const error = ref("");
let diff: editor.IStandaloneDiffEditor | undefined;
let resize: ResizeObserver | undefined;
let disposed = false;

function layout() {
  if (disposed || !diff || !container.value) return;
  const width = container.value.clientWidth;
  const height = container.value.clientHeight;
  if (width > 0 && height > 0) {
    diff.layout({ width, height });
  }
}

onMounted(async () => {
  try {
    const api = await loadMonaco();
    const [original, modified] = await Promise.all([textModelFor(props.left), textModelFor(props.right)]);
    if (disposed || !container.value) return;
    diff = api.editor.createDiffEditor(container.value, {
      theme: "vs-dark", originalEditable: !props.locked, readOnly: props.locked,
      renderSideBySide: true, useInlineViewWhenSpaceIsLimited: false,
      ignoreTrimWhitespace: false, renderIndicators: true, renderOverviewRuler: true,
      automaticLayout: false, scrollBeyondLastLine: false, fontSize: 13,
    });
    diff.setModel({ original, modified });
    diff.getOriginalEditor().onDidFocusEditorText(() => emit("focus", "left"));
    diff.getModifiedEditor().onDidFocusEditorText(() => emit("focus", "right"));
    resize = new ResizeObserver((entries) => {
      if (disposed) return;
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        if (width > 0 && height > 0) {
          diff?.layout({ width, height });
        }
      }
    });
    resize.observe(container.value);
    nextTick(() => layout());
  } catch (cause) { if (!disposed) error.value = errorMessage(cause); }
});

onActivated(() => {
  nextTick(() => {
    layout();
    requestAnimationFrame(() => layout());
  });
});

watch(() => props.locked, (locked) => diff?.updateOptions({ readOnly: locked, originalEditable: !locked }));
onBeforeUnmount(() => { disposed = true; resize?.disconnect(); diff?.dispose(); });
</script>

<template>
  <section class="flex size-full min-h-0 flex-col bg-inverted text-white/90">
    <div class="flex shrink-0 border-b border-muted text-sm">
      <div v-for="side in (['left', 'right'] as const)" :key="side" class="flex min-w-0 flex-1 items-center gap-2 px-3 py-2">
        <span class="min-w-0 flex-1 truncate" :title="props[side].file?.path">{{ props[side].file?.name }}{{ props[side].dirty ? ' ●' : '' }}</span>
        <UButton v-if="props[side].dirty" size="xs" variant="ghost" icon="i-lucide-save" :disabled="locked" @click="emit('save', side)">保存</UButton>
      </div>
    </div>
    <UAlert v-if="error" class="m-3" color="error" title="无法打开文本对比" :description="error" />
    <div v-else ref="container" class="relative min-h-0 flex-1 overflow-hidden" />
    <div class="flex shrink-0 items-center justify-end gap-2 border-t border-muted px-3 py-1">
      <span class="mr-auto text-xs text-muted">文本差异 · 两侧可独立编辑与保存</span>
      <UButton size="xs" variant="ghost" @click="diff?.goToDiff('previous')">上一处差异</UButton>
      <UButton size="xs" variant="ghost" @click="diff?.goToDiff('next')">下一处差异</UButton>
    </div>
  </section>
</template>
