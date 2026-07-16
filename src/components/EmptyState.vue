<script setup lang="ts">
const emit = defineEmits<{ select: [] }>();
defineProps<{ dragActive?: boolean }>();

const formats = ["图片", "文本", "JSON / YAML", "Markdown"];
</script>

<template>
  <section class="w-full max-w-2xl text-center" aria-labelledby="empty-title">
    <div
      class="group relative overflow-hidden rounded-3xl border border-dashed px-6 py-14 shadow-sm transition sm:px-12 sm:py-16"
      :class="dragActive ? 'border-primary bg-primary/5 ring-4 ring-primary/10' : 'border-accented bg-elevated'"
    >
      <div
        class="pointer-events-none absolute inset-x-20 top-0 h-32 rounded-full bg-primary/10 blur-3xl"
      />

      <div class="relative flex flex-col items-center">
        <div
          class="mb-6 flex size-16 items-center justify-center rounded-2xl bg-primary/10 text-primary ring-1 ring-primary/15"
        >
          <UIcon name="i-lucide-file-search" class="size-8" />
        </div>

        <h1 id="empty-title" class="text-2xl font-semibold tracking-tight text-highlighted">
          {{ dragActive ? "松开即可打开文件" : "打开文件，快速查看" }}
        </h1>
        <p class="mt-3 max-w-md text-sm leading-6 text-muted">
          将文件拖到这里，或从电脑中选择一个文件。KView 会在本机完成读取与展示。
        </p>

        <UButton
          class="mt-7"
          icon="i-lucide-folder-open"
          label="选择文件"
          size="lg"
          @click="emit('select')"
        />

        <div class="mt-8 flex flex-wrap justify-center gap-2" aria-label="主要支持格式">
          <UBadge
            v-for="format in formats"
            :key="format"
            color="neutral"
            variant="subtle"
          >
            {{ format }}
          </UBadge>
        </div>
      </div>
    </div>

    <p class="mt-5 flex items-center justify-center gap-1.5 text-xs text-dimmed">
      <UIcon name="i-lucide-shield-check" class="size-3.5" />
      无需上传，不会读取你未选择的文件
    </p>
  </section>
</template>
