<script setup lang="ts">
import { onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { OpenedFile } from "../types/document";
import { desktopInvoke } from "../services/desktop-api";
import { recordDiagnostic } from "../services/diagnostics";
import ViewerFooter from "../components/ViewerFooter.vue";
import TextViewer from "./TextViewer.vue";

const props = defineProps<{ file: OpenedFile; content: string; readOnly: boolean }>();
const emit = defineEmits<{ "update:content": [value: string] }>();
interface PreviewHandle { sessionId: string; url: string }
interface ResourceError { sessionId: string; message: string }

const mode = ref<"text" | "preview">("text");
const wordWrap = ref(true);
const unicodeHighlight = ref(false);
const active = ref(true);
const previewUrl = ref("");
const error = ref("");
const viewModes = [
  { label: "源码", value: "text", icon: "i-lucide-code" },
  { label: "页面", value: "preview", icon: "i-lucide-eye" },
];
let session: PreviewHandle | undefined;
let sessionDocument = "";
let generation = 0;
let pending = Promise.resolve();
let unlisten: UnlistenFn | undefined;
let disposed = false;

function reportError(cause: unknown) {
  error.value = cause instanceof Error ? cause.message : String(cause);
  recordDiagnostic("viewer", "html-preview-failed");
}

async function closeSession() {
  if (!session) return;
  const closing = session;
  await desktopInvoke("close_html_preview", { sessionId: closing.sessionId });
  session = undefined;
  sessionDocument = "";
}

function previewContent(file: OpenedFile, content: string) {
  if (file.source !== "remote") return content;
  const document = new DOMParser().parseFromString(content, "text/html");
  const base = document.querySelector("base[href]");
  if (base) base.setAttribute("href", new URL(base.getAttribute("href")!, file.path).href);
  else {
    const element = document.createElement("base");
    element.href = file.path;
    document.head.prepend(element);
  }
  return `${document.doctype ? new XMLSerializer().serializeToString(document.doctype) : ""}\n${document.documentElement.outerHTML}`;
}

function syncPreview() {
  const ticket = ++generation;
  const visible = active.value && mode.value === "preview" && !disposed;
  const file = { ...props.file };
  const content = props.content;
  previewUrl.value = "";
  error.value = "";
  // 串行处理会话，快速切换时也要关闭已经创建的旧会话。
  pending = pending.then(async () => {
    if (ticket !== generation) return;
    if (!active.value || disposed) { await closeSession(); return; }
    const documentId = JSON.stringify([file.source, file.path, file.modifiedAt]);
    if (session && sessionDocument !== documentId) await closeSession();
    if (!visible) return;
    const html = previewContent(file, content);
    if (session) {
      await desktopInvoke("update_html_preview", { sessionId: session.sessionId, content: html });
    } else {
      session = await desktopInvoke<PreviewHandle>("create_html_preview", {
        source: file.source, path: file.path, content: html,
      });
      sessionDocument = documentId;
    }
    if (ticket === generation) previewUrl.value = `${session.url}?revision=${ticket}`;
  }).catch((cause: unknown) => {
    if (ticket === generation && !disposed) reportError(cause);
    else recordDiagnostic("viewer", "html-preview-cleanup-failed");
  });
}

watch([active, mode, () => props.file.source, () => props.file.path, () => props.file.modifiedAt, () => props.content], syncPreview, { immediate: true });
onActivated(() => { active.value = true; });
onDeactivated(() => { active.value = false; });
onMounted(() => {
  void listen<ResourceError>("html-preview-error", ({ payload }) => {
    if (payload.sessionId === session?.sessionId && active.value && mode.value === "preview") {
      error.value = payload.message;
    }
  }).then((stop) => {
    if (disposed) stop();
    else unlisten = stop;
  }).catch(reportError);
});
onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
  syncPreview();
});

function updateMode(value: string | number) {
  if (value === "text" || value === "preview") mode.value = value;
}
</script>

<template>
  <div class="flex size-full min-h-0 flex-col bg-default">
    <TextViewer
      v-if="mode === 'text'"
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
      <div v-if="error" class="shrink-0 bg-error/10 px-3 py-2 text-sm text-error" role="alert">
        {{ error }}
      </div>
      <iframe
        v-if="active && previewUrl"
        :key="previewUrl"
        :src="previewUrl"
        title="HTML 页面预览"
        sandbox="allow-scripts"
        referrerpolicy="no-referrer"
        class="min-h-0 w-full flex-1 border-0 bg-white"
        @error="reportError(new Error('无法加载 HTML 页面预览'))"
      />
      <div v-else-if="!error" class="grid min-h-0 flex-1 place-items-center text-sm text-muted">
        正在加载页面…
      </div>
    </div>
    <ViewerFooter :file-size="file.size">
      <template #status>
        {{ file.extension.toUpperCase() }}
        <template v-if="file.encoding"> · {{ file.encoding.toUpperCase() }}</template>
        <template v-if="file.lineEnding"> · {{ file.lineEnding.toUpperCase() }}</template>
        · {{ readOnly ? "只读" : "可编辑" }}
      </template>
      <template #actions>
        <div v-if="mode === 'text'" class="flex items-center gap-3">
          <USwitch v-model="wordWrap" size="xs" label="自动换行" />
          <USwitch v-model="unicodeHighlight" size="xs" label="字符高亮" />
        </div>
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
