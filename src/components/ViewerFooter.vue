<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  fileSize?: number;
  notice?: string | readonly string[];
}>();

const fileSizeLabel = computed(() => {
  if (props.fileSize === undefined) return "";
  if (props.fileSize < 1024) return `${props.fileSize} B`;
  if (props.fileSize < 1024 ** 2) return `${(props.fileSize / 1024).toFixed(1)} KiB`;
  if (props.fileSize < 1024 ** 3) return `${(props.fileSize / 1024 ** 2).toFixed(2)} MiB`;
  return `${(props.fileSize / 1024 ** 3).toFixed(2)} GiB`;
});

const noticeLines = computed(() => (
  Array.isArray(props.notice) ? props.notice : props.notice ? [props.notice] : []
));
</script>

<template>
  <footer class="viewer-footer flex h-9 shrink-0 items-center gap-3 border-t border-muted bg-elevated px-3 text-xs text-muted">
    <div class="flex min-w-0 flex-1 items-center gap-1.5">
      <div class="min-w-0 truncate"><slot name="status" /></div>
      <span v-if="fileSizeLabel" class="shrink-0">· {{ fileSizeLabel }}</span>
      <UTooltip
        v-if="noticeLines.length"
        :delay-duration="300"
        :content="{ side: 'top', align: 'start', sideOffset: 8 }"
        :ui="{ content: 'compatibility-popover' }"
        arrow
      >
        <button
          type="button"
          class="grid size-5 shrink-0 place-items-center rounded text-amber-500 hover:bg-amber-500/10 focus-visible:outline-2 focus-visible:outline-amber-500"
          :aria-label="noticeLines.join('；')"
        >
          <UIcon name="i-lucide-info" class="size-3.5" />
        </button>
        <template #content>
          <div class="max-h-[min(20rem,calc(100vh-2rem))] w-[min(24rem,calc(100vw-2rem))] overflow-y-auto whitespace-normal break-words text-xs leading-5 text-default">
            <p v-for="line in noticeLines" :key="line">{{ line }}</p>
          </div>
        </template>
      </UTooltip>
    </div>
    <div class="flex shrink-0 items-center gap-0.5 whitespace-nowrap">
      <slot name="actions" />
    </div>
    <div v-if="$slots.more" class="flex shrink-0 items-center border-l border-muted/70 pl-1">
      <slot name="more" />
    </div>
  </footer>
</template>
