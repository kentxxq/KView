<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { normalizePublicUrl } from "../utils/url";

const emit = defineEmits<{
  select: [];
  openUrl: [];
  pasteUrl: [url: string];
  pastePath: [path: string];
  pasteImage: [file: File];
  pasteText: [text: string];
}>();
defineProps<{ dragActive?: boolean }>();

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || !!target.closest("input, textarea, [contenteditable='true']");
}

function handlePaste(event: ClipboardEvent) {
  if (isEditableTarget(event.target)) return;
  const clipboard = event.clipboardData;
  if (!clipboard) return;

  const imageItem = Array.from(clipboard.items).find(
    (item) => item.kind === "file" && item.type.startsWith("image/"),
  );
  const image =
    imageItem?.getAsFile() ??
    Array.from(clipboard.files).find((file) => file.type.startsWith("image/"));
  if (image) {
    event.preventDefault();
    emit("pasteImage", image);
    return;
  }

  const text = clipboard.getData("text/plain");
  if (!text.trim()) return;
  const trimmed = text.trim();
  const path =
    trimmed.startsWith('"') && trimmed.endsWith('"') ? trimmed.slice(1, -1) : trimmed;
  if (!/[\r\n]/.test(path) && /^(?:[a-z]:[\\/]|\\\\|\/|%[^%]+%)/i.test(path)) {
    event.preventDefault();
    emit("pastePath", path);
    return;
  }
  try {
    const url = normalizePublicUrl(text.trim());
    event.preventDefault();
    emit("pasteUrl", url);
  } catch {
    event.preventDefault();
    emit("pasteText", text);
  }
}

onMounted(() => window.addEventListener("paste", handlePaste));
onBeforeUnmount(() => window.removeEventListener("paste", handlePaste));
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
          将文件拖到这里、从电脑中选择，或直接粘贴文本、URL 和剪贴板图片
        </p>
      </template>
      <template #actions>
        <div class="flex flex-wrap justify-center gap-3">
          <UButton icon="i-lucide-folder-open" label="选择文件" size="lg" @click="emit('select')" />
          <UButton
            color="neutral"
            variant="soft"
            icon="i-lucide-link"
            label="打开 URL"
            size="lg"
            @click="emit('openUrl')"
          />
        </div>
      </template>
    </UFileUpload>
  </section>
</template>
