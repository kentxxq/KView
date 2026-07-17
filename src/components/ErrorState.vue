<script setup lang="ts">
import type { DocumentError } from "../types/document";

defineProps<{ error: DocumentError; canOpenHex?: boolean }>();
defineEmits<{ dismiss: []; openHex: [] }>();
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
      <div class="mt-7 flex justify-center gap-2">
        <UButton
          v-if="error.recoverable"
          color="neutral"
          variant="outline"
          icon="i-lucide-arrow-left"
          label="返回"
          @click="$emit('dismiss')"
        />
        <UButton
          v-if="canOpenHex"
          icon="i-lucide-binary"
          label="以 Hex 打开"
          @click="$emit('openHex')"
        />
      </div>
    </div>
  </section>
</template>
