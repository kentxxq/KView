<script setup lang="ts">
const emit = defineEmits<{ select: [] }>();
defineProps<{ dragActive?: boolean }>();
</script>

<template>
  <section class="w-full max-w-2xl text-center" aria-labelledby="empty-title">
    <UFileUpload
      class="w-full"
      variant="area"
      size="xl"
      icon="i-lucide-file-search"
      :dropzone="false"
      :interactive="false"
      :preview="false"
      :highlight="dragActive"
      :ui="{
        base: 'min-h-80 overflow-hidden rounded-3xl border-dashed bg-elevated shadow-sm transition',
        wrapper: 'px-6 py-14 sm:px-12 sm:py-16',
        icon: 'size-12 text-primary',
        actions: 'mt-7',
      }"
    >
      <template #label>
        <h1 id="empty-title" class="text-2xl font-semibold tracking-tight text-highlighted">
          {{ dragActive ? "松开即可打开文件" : "打开文件，快速查看" }}
        </h1>
      </template>
      <template #description>
        <p class="mt-3 max-w-md text-sm leading-6 text-muted">
          将文件拖到这里，或从电脑中选择一个文件
        </p>
      </template>
      <template #actions>
        <UButton
          icon="i-lucide-folder-open"
          label="选择文件"
          size="lg"
          @click="emit('select')"
        />
      </template>
    </UFileUpload>
  </section>
</template>
