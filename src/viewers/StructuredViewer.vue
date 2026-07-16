<script setup lang="ts">
import { computed, ref } from "vue";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";
import type { OpenedFile } from "../types/document";
import TextViewer from "./TextViewer.vue";
import TreeNode from "./TreeNode.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const emit = defineEmits<{ "update:content": [value: string] }>();

const mode = ref<"source" | "tree">("source");
const search = ref("");
const selectedPath = ref("$");

const parsed = computed(() => {
  try {
    return {
      value: props.file.extension === "json" ? JSON.parse(props.content) : parseYaml(props.content),
      error: "",
    };
  } catch (error) {
    return { value: null, error: error instanceof Error ? error.message : String(error) };
  }
});

function formatSource() {
  if (parsed.value.error || props.readOnly) return;
  const formatted =
    props.file.extension === "json"
      ? `${JSON.stringify(parsed.value.value, null, 2)}\n`
      : stringifyYaml(parsed.value.value);
  emit("update:content", formatted);
}

async function copySelectedPath() {
  await navigator.clipboard.writeText(selectedPath.value);
}
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-default">
    <div class="flex h-12 shrink-0 items-center justify-between border-b border-muted px-4">
      <div class="flex items-center gap-1">
        <UButton
          color="neutral"
          :variant="mode === 'source' ? 'soft' : 'ghost'"
          icon="i-lucide-code-xml"
          label="源码"
          @click="mode = 'source'"
        />
        <UButton
          color="neutral"
          :variant="mode === 'tree' ? 'soft' : 'ghost'"
          icon="i-lucide-list-tree"
          label="树形"
          @click="mode = 'tree'"
        />
      </div>
      <UButton
        :disabled="readOnly || !!parsed.error"
        color="neutral"
        variant="ghost"
        icon="i-lucide-align-left"
        label="格式化"
        @click="formatSource"
      />
    </div>

    <TextViewer
      v-if="mode === 'source'"
      class="min-h-0 flex-1"
      :file="file"
      :content="content"
      :read-only="readOnly"
      @update:content="emit('update:content', $event)"
    />

    <div v-else class="flex min-h-0 flex-1 flex-col">
      <UAlert
        v-if="parsed.error"
        class="m-4"
        color="error"
        variant="subtle"
        title="无法生成树形视图"
        :description="parsed.error"
      />
      <template v-else>
        <div class="flex items-center gap-2 border-b border-muted p-3">
          <UInput v-model="search" class="flex-1" icon="i-lucide-search" placeholder="搜索节点或路径" />
          <code class="max-w-64 truncate rounded bg-muted px-2 py-1 text-xs text-toned">{{ selectedPath }}</code>
          <UButton color="neutral" variant="ghost" icon="i-lucide-copy" @click="copySelectedPath" />
        </div>
        <div class="min-h-0 flex-1 overflow-auto p-4">
          <TreeNode
            :value="parsed.value"
            name="$"
            path="$"
            :search="search"
            @select-path="selectedPath = $event"
          />
        </div>
      </template>
    </div>
  </div>
</template>
