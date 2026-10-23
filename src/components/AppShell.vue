<script setup lang="ts">
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onMounted, ref } from "vue";
import { desktopInvoke } from "../services/desktop-api";
import { errorMessage } from "../utils/error";
import { useToast } from "@nuxt/ui/composables";

const pinned = ref(false);
const pinPending = ref(false);
const toast = useToast();
onMounted(async () => {
  if (!isTauri()) return;
  try { pinned.value = await desktopInvoke<boolean>("get_window_pin"); }
  catch (error) { toast.add({ title: "无法读取置顶状态", description: errorMessage(error), color: "error" }); }
});
async function togglePin() {
  if (!isTauri() || pinPending.value) return;
  pinPending.value = true;
  try { pinned.value = await desktopInvoke<boolean>("set_window_pin", { pinned: !pinned.value }); }
  catch (error) { toast.add({ title: "无法切换置顶", description: errorMessage(error), color: "error" }); }
  finally { pinPending.value = false; }
}

defineProps<{ immersive?: boolean }>();
const emit = defineEmits<{ "request-close": [] }>();
const isDevelopment = import.meta.env.DEV;

function withWindow(action: (window: ReturnType<typeof getCurrentWindow>) => Promise<void>) {
  if (isTauri()) void action(getCurrentWindow());
}

function minimizeWindow() {
  withWindow((window) => window.minimize());
}

function toggleMaximizeWindow() {
  withWindow((window) => window.toggleMaximize());
}

function closeWindow() {
  emit("request-close");
}
</script>

<template>
  <div class="flex h-screen min-h-0 flex-col bg-default text-default">
    <header
      class="app-titlebar flex h-9 shrink-0 items-stretch border-b border-muted bg-elevated/95"
    >
      <div class="flex min-w-24 shrink-0 items-center gap-2 px-2.5" data-tauri-drag-region>
        <div class="flex size-5 items-center justify-center rounded bg-primary text-inverted">
          <UIcon name="i-lucide-scan-eye" class="size-3.5" />
        </div>
        <span class="select-none text-xs font-semibold text-highlighted">KView</span>
        <span v-if="isDevelopment" class="select-none rounded bg-amber-500/20 px-1 py-0.5 text-[9px] font-bold leading-none text-amber-700 dark:text-amber-400">DEV</span>
      </div>

      <div class="min-w-0 flex-1" data-tauri-drag-region>
        <slot name="title-content" />
      </div>

      <div class="flex shrink-0 items-stretch">
        <div class="flex items-center gap-0.5 border-l border-muted/60 px-1.5">
          <slot name="actions" />
        </div>
        <div class="flex items-stretch border-l border-muted/60">
          <UTooltip :text="pinned ? '取消置顶' : '置顶'" :delay-duration="500">
            <button class="titlebar-button" :class="{ 'text-primary bg-primary/10': pinned }" type="button" :aria-label="pinned ? '取消置顶' : '置顶'" :aria-pressed="pinned" :disabled="pinPending" @click="togglePin">
              <UIcon :name="pinned ? 'i-lucide-pin-off' : 'i-lucide-pin'" class="size-3.5" />
            </button>
          </UTooltip>
          <UTooltip text="最小化" :delay-duration="500">
            <button
              class="titlebar-button"
              type="button"
              aria-label="最小化"
              @click="minimizeWindow"
            >
              <UIcon name="i-lucide-minus" class="size-3.5" />
            </button>
          </UTooltip>
          <UTooltip text="最大化或还原" :delay-duration="500">
            <button
              class="titlebar-button"
              type="button"
              aria-label="最大化或还原"
              @click="toggleMaximizeWindow"
            >
              <UIcon name="i-lucide-square" class="size-3" />
            </button>
          </UTooltip>
          <UTooltip text="关闭" :delay-duration="500">
            <button
              class="titlebar-button titlebar-close"
              type="button"
              aria-label="关闭"
              @click="closeWindow"
            >
              <UIcon name="i-lucide-x" class="size-3.5" />
            </button>
          </UTooltip>
        </div>
      </div>
    </header>

    <main
      class="flex min-h-0 flex-1"
      :class="
        immersive ? 'items-stretch justify-stretch' : 'items-center justify-center p-6 sm:p-10'
      "
    >
      <slot />
    </main>
  </div>
</template>
