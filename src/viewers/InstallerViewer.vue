<script setup lang="ts">
import { computed } from "vue";
import type { InstallerDocument, OpenedFile } from "../types/document";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile; content: string }>();

const installer = computed(() => JSON.parse(props.content) as InstallerDocument);

const sizeLabel = computed(() => {
  const bytes = props.file.size;
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GiB`;
});

const modifiedLabel = computed(() =>
  new Intl.DateTimeFormat("zh-CN", {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(new Date(props.file.modifiedAt)),
);

const details = computed(() => [
  { label: "文件类型", value: installer.value.format, icon: "i-lucide-package" },
  { label: "版本", value: installer.value.version, icon: "i-lucide-tag" },
  { label: "系统架构", value: installer.value.architecture, icon: "i-lucide-cpu" },
  { label: "文件大小", value: sizeLabel.value, icon: "i-lucide-hard-drive" },
  { label: "修改时间", value: modifiedLabel.value, icon: "i-lucide-clock-3" },
  { label: "发布者", value: installer.value.publisher, icon: "i-lucide-building-2" },
]);
</script>

<template>
  <section class="flex size-full min-h-0 flex-col bg-muted">
    <div class="min-h-0 flex-1 overflow-auto p-6 sm:p-10">
      <div
        class="mx-auto max-w-3xl overflow-hidden rounded-3xl border border-muted bg-elevated shadow-sm"
      >
        <div
          class="flex flex-col items-center gap-5 border-b border-muted px-6 py-8 text-center sm:flex-row sm:px-9 sm:text-left"
        >
          <div
            class="flex size-24 shrink-0 items-center justify-center overflow-hidden rounded-2xl bg-muted shadow-inner"
          >
            <img
              v-if="installer.logoDataUrl"
              :src="installer.logoDataUrl"
              :alt="`${installer.name || file.name} Logo`"
              class="size-20 object-contain"
            />
            <UIcon v-else name="i-lucide-app-window" class="size-11 text-dimmed" />
          </div>
          <div class="min-w-0">
            <UBadge color="neutral" variant="subtle">{{ installer.format }}</UBadge>
            <h1 class="mt-3 break-words text-2xl font-semibold text-highlighted">
              {{ installer.name || file.name }}
            </h1>
            <p
              v-if="installer.description && installer.description !== installer.name"
              class="mt-2 text-sm leading-6 text-muted"
            >
              {{ installer.description }}
            </p>
          </div>
        </div>

        <dl class="grid gap-px bg-muted sm:grid-cols-2">
          <div
            v-for="item in details"
            :key="item.label"
            class="flex min-h-24 gap-3 bg-elevated px-6 py-5"
          >
            <UIcon :name="item.icon" class="mt-0.5 size-5 shrink-0 text-dimmed" />
            <div class="min-w-0">
              <dt class="text-xs text-muted">{{ item.label }}</dt>
              <dd class="mt-1 break-words text-sm font-medium text-highlighted">
                {{ item.value || "未提供" }}
              </dd>
            </div>
          </div>
        </dl>
      </div>
    </div>
    <ViewerFooter :file-size="file.size">
      <template #status
        >{{ installer.format }} · {{ installer.architecture || "未知架构" }} · 只读</template
      >
    </ViewerFooter>
  </section>
</template>
