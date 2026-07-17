<script setup lang="ts">
import { computed } from "vue";
import type { OpenedFile } from "../types/document";

const props = defineProps<{ file: OpenedFile }>();
defineEmits<{ open: [] }>();

const kindLabels: Record<string, string> = {
  image: "图片",
  text: "文本",
  markdown: "Markdown",
  json: "JSON",
  yaml: "YAML",
  pdf: "PDF",
  docx: "DOCX",
  xlsx: "XLSX",
  archive: "压缩包",
  video: "视频",
  installer: "安装包",
  unsupported: "未知格式",
};

const sizeLabel = computed(() => {
  const bytes = props.file.size;
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
});
</script>

<template>
  <section class="w-full max-w-xl text-center" aria-labelledby="ready-title">
    <div class="rounded-3xl border border-muted bg-elevated px-8 py-12 shadow-sm">
      <div
        class="mx-auto mb-5 flex size-16 items-center justify-center rounded-2xl bg-primary/10 text-primary"
      >
        <UIcon name="i-lucide-file-check" class="size-8" />
      </div>
      <UBadge color="success" variant="subtle">文件已接收</UBadge>
      <h1 id="ready-title" class="mt-4 truncate text-xl font-semibold text-highlighted">
        {{ file.name }}
      </h1>
      <UTooltip :text="file.path" :delay-duration="500" :content="{ side: 'bottom', sideOffset: 6 }" :ui="{ content: 'max-w-md break-all font-mono' }">
        <p class="mx-auto mt-2 max-w-md truncate font-mono text-xs text-dimmed">{{ file.path }}</p>
      </UTooltip>

      <dl class="mx-auto mt-7 grid max-w-md grid-cols-3 gap-3 text-left">
        <div class="rounded-xl bg-muted px-4 py-3">
          <dt class="text-xs text-muted">类型</dt>
          <dd class="mt-1 text-sm font-medium text-highlighted">
            {{ kindLabels[file.kind] ?? "未知格式" }}
          </dd>
        </div>
        <div class="rounded-xl bg-muted px-4 py-3">
          <dt class="text-xs text-muted">扩展名</dt>
          <dd class="mt-1 text-sm font-medium uppercase text-highlighted">
            {{ file.extension || "—" }}
          </dd>
        </div>
        <div class="rounded-xl bg-muted px-4 py-3">
          <dt class="text-xs text-muted">大小</dt>
          <dd class="mt-1 text-sm font-medium text-highlighted">{{ sizeLabel }}</dd>
        </div>
      </dl>

      <UAlert
        class="mt-6 text-left"
        color="info"
        variant="subtle"
        icon="i-lucide-info"
        title="文件打开链路已完成"
        description="具体内容 Viewer 将在下一个里程碑接入。"
      />

      <UButton
        class="mt-7"
        color="neutral"
        variant="outline"
        icon="i-lucide-folder-open"
        label="选择其他文件"
        @click="$emit('open')"
      />
    </div>
  </section>
</template>
