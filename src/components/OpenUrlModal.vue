<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { normalizePublicUrl } from "../utils/url";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{
  "update:open": [value: boolean];
  openUrl: [url: string];
}>();

const value = ref("");
const error = ref("");
const input = ref<{ inputRef?: Readonly<{ value: HTMLInputElement | null }> }>();

function submit() {
  error.value = "";
  try {
    const url = normalizePublicUrl(value.value);
    emit("openUrl", url);
    emit("update:open", false);
    value.value = "";
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : "请输入有效的 URL";
  }
}

watch(
  () => props.open,
  async (open) => {
    if (!open) return;
    error.value = "";
    await nextTick();
    input.value?.inputRef?.value?.focus();
  },
);
</script>

<template>
  <UModal
    :open="open"
    title="打开 URL"
    description="支持公开的 HTTP/HTTPS 图片、PDF 和视频地址"
    @update:open="emit('update:open', $event)"
  >
    <template #body>
      <UFormField label="URL" :error="error">
        <UInput
          ref="input"
          v-model="value"
          class="w-full"
          type="url"
          icon="i-lucide-link"
          placeholder="https://example.com/file.pdf"
          autocomplete="off"
          autofocus
          @keyup.enter="submit"
        />
      </UFormField>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton color="neutral" variant="ghost" label="取消" @click="emit('update:open', false)" />
        <UButton icon="i-lucide-external-link" label="打开" :disabled="!value.trim()" @click="submit" />
      </div>
    </template>
  </UModal>
</template>
