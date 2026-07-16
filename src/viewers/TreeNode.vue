<script setup lang="ts">
import { computed, ref } from "vue";

defineOptions({ name: "TreeNode" });

const props = defineProps<{
  value: unknown;
  name: string;
  path: string;
  search: string;
  depth?: number;
}>();

const emit = defineEmits<{ selectPath: [path: string] }>();
const expanded = ref((props.depth ?? 0) < 2);

const entries = computed(() => {
  if (!props.value || typeof props.value !== "object") return [];
  return Object.entries(props.value as Record<string, unknown>);
});

const isBranch = computed(() => entries.value.length > 0);
const matches = computed(() => {
  const query = props.search.trim().toLowerCase();
  if (!query) return true;
  if (`${props.name} ${props.path}`.toLowerCase().includes(query)) return true;
  try {
    return JSON.stringify(props.value).toLowerCase().includes(query);
  } catch {
    return false;
  }
});

function childPath(key: string) {
  return Array.isArray(props.value)
    ? `${props.path}[${key}]`
    : `${props.path}[${JSON.stringify(key)}]`;
}

function primitiveLabel(value: unknown) {
  if (typeof value === "string") return JSON.stringify(value);
  if (value === null) return "null";
  return String(value);
}

function branchLabel(value: unknown, length: number) {
  return Array.isArray(value) ? `[${length}]` : `{${length}}`;
}
</script>

<template>
  <div v-if="matches" class="font-mono text-xs leading-6">
    <button
      class="group flex w-full items-center gap-1 rounded px-1 text-left hover:bg-accented"
      type="button"
      @click="isBranch ? (expanded = !expanded) : emit('selectPath', path)"
      @dblclick.stop="emit('selectPath', path)"
    >
      <UIcon
        v-if="isBranch"
        name="i-lucide-chevron-right"
        class="size-3 shrink-0 text-dimmed transition-transform"
        :class="expanded && 'rotate-90'"
      />
      <span v-else class="inline-block size-3 shrink-0" />
      <span class="text-primary">{{ name }}</span>
      <span class="text-dimmed">:</span>
      <span v-if="isBranch" class="text-muted">
        {{ branchLabel(value, entries.length) }}
      </span>
      <span v-else class="truncate text-toned">{{ primitiveLabel(value) }}</span>
    </button>

    <div v-if="isBranch && expanded" class="ml-4 border-l border-muted pl-2">
      <TreeNode
        v-for="([key, child]) in entries"
        :key="key"
        :value="child"
        :name="key"
        :path="childPath(key)"
        :search="search"
        :depth="(depth ?? 0) + 1"
        @select-path="emit('selectPath', $event)"
      />
    </div>
  </div>
</template>
