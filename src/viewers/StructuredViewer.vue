<script setup lang="ts">
import { computed, ref } from "vue";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";
import type { OpenedFile } from "../types/document";
import TextViewer from "./TextViewer.vue";
import TreeNode from "./TreeNode.vue";
import ViewerFooter from "../components/ViewerFooter.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const emit = defineEmits<{ "update:content": [value: string] }>();

const mode = ref<"source" | "tree">("source");
const wordWrap = ref(true);
const unicodeHighlight = ref(false);
const search = ref("");
const selectedPath = ref("$");
const viewModes = [
  { label: "源码", value: "source", icon: "i-lucide-code-xml" },
  { label: "树形", value: "tree", icon: "i-lucide-list-tree" },
];

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

function updateMode(value: string | number) {
  if (value === "source" || value === "tree") mode.value = value;
}
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-default">
    <TextViewer
      v-if="mode === 'source'"
      class="min-h-0 flex-1"
      :file="file"
      :content="content"
      :read-only="readOnly"
      v-model:word-wrap="wordWrap"
      v-model:unicode-highlight="unicodeHighlight"
      embedded
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

    <ViewerFooter :file-size="file.size">
      <template #status>
        {{ file.extension.toUpperCase() }}
        <template v-if="file.encoding"> · {{ file.encoding.toUpperCase() }}</template>
        <template v-if="file.lineEnding"> · {{ file.lineEnding.toUpperCase() }}</template>
      </template>
      <template #actions>
        <template v-if="mode === 'source'">
          <div>
            <UButton
              class="viewer-footer-button"
              color="neutral"
              size="xs"
              variant="ghost"
              icon="i-lucide-align-left"
              label="格式化"
              :disabled="readOnly || !!parsed.error"
              @click="formatSource"
            />
          </div>
          <div class="ml-1 flex items-center gap-3 border-l border-muted/70 pl-2">
            <USwitch v-model="wordWrap" size="xs" label="自动换行" />
            <USwitch v-model="unicodeHighlight" size="xs" label="字符高亮" />
          </div>
        </template>
        <UPopover
          v-else
          :content="{ side: 'top', align: 'end', sideOffset: 8 }"
          :ui="{ content: 'compatibility-popover' }"
        >
          <UButton
            class="viewer-footer-button"
            color="neutral"
            size="xs"
            variant="ghost"
            icon="i-lucide-search"
            label="查找"
          />
          <template #content>
            <div class="w-80 space-y-2">
              <UInput
                v-model="search"
                icon="i-lucide-search"
                placeholder="搜索节点或路径"
                autofocus
              />
              <div class="flex items-center gap-2">
                <code
                  class="min-w-0 flex-1 truncate rounded bg-muted px-2 py-1 text-xs text-toned"
                  >{{ selectedPath }}</code
                >
                <UTooltip text="复制路径" :delay-duration="500"
                  ><UButton
                    color="neutral"
                    variant="ghost"
                    size="xs"
                    icon="i-lucide-copy"
                    aria-label="复制路径"
                    @click="copySelectedPath"
                /></UTooltip>
              </div>
            </div>
          </template>
        </UPopover>
      </template>
      <template #viewSwitch>
        <UTabs
          :model-value="mode"
          :items="viewModes"
          :content="false"
          activation-mode="automatic"
          color="neutral"
          variant="pill"
          size="xs"
          :ui="{ root: 'w-auto gap-0', list: 'w-auto p-0.5', trigger: 'py-0.5' }"
          @update:model-value="updateMode"
        />
      </template>
    </ViewerFooter>
  </div>
</template>
