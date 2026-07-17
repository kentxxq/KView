<template>
  <footer class="viewer-footer flex h-9 shrink-0 items-center gap-3 border-t border-muted bg-elevated px-3 text-xs text-muted">
    <div class="min-w-0 flex-1 truncate">
      <slot name="status" />
    </div>
    <div class="flex shrink-0 items-center gap-0.5 whitespace-nowrap">
      <slot name="actions" />
    </div>
    <div v-if="$slots.more" class="flex shrink-0 items-center border-l border-muted/70 pl-1">
      <slot name="more" />
    </div>
  </footer>
</template>
