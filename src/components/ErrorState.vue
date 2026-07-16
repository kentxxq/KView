<script setup lang="ts">
import type { DocumentError } from "../types/document";

defineProps<{ error: DocumentError }>();
defineEmits<{ dismiss: [] }>();
</script>

<template>
  <section class="w-full max-w-lg text-center" role="alert" aria-labelledby="error-title">
    <div class="rounded-3xl border border-muted bg-elevated px-8 py-12 shadow-sm">
      <div
        class="mx-auto mb-5 flex size-14 items-center justify-center rounded-2xl bg-error/10 text-error"
      >
        <UIcon name="i-lucide-file-search" class="size-7" />
      </div>
      <h1 id="error-title" class="text-xl font-semibold text-highlighted">
        {{ error.title }}
      </h1>
      <p class="mt-3 text-sm leading-6 text-muted">{{ error.message }}</p>
      <p class="mt-2 font-mono text-xs text-dimmed">错误代码：{{ error.code }}</p>
      <UButton
        v-if="error.recoverable"
        class="mt-7"
        color="neutral"
        variant="outline"
        icon="i-lucide-arrow-left"
        label="返回"
        @click="$emit('dismiss')"
      />
    </div>
  </section>
</template>
