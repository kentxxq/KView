<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { useToast } from "@nuxt/ui/composables";
import { isTauri } from "@tauri-apps/api/core";
import { desktopInvoke as invoke } from "../services/desktop-api";
import { useSettingsStore, type AppSettings } from "../state/settings";
import { associationGroupsForPlatform } from "../domain/formats";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();
const toast = useToast();
const appSettings = useSettingsStore();
const saving = ref(false);
const openingFileAssociations = ref(false);
const openingLogDirectory = ref(false);
function settingsSnapshot(settings: AppSettings): AppSettings {
  return {
    theme: settings.theme,
    residencyMode: settings.residencyMode,
    closeBehavior: settings.closeBehavior,
    autostart: settings.autostart,
    largeFileThresholdMb: settings.largeFileThresholdMb,
  };
}

const form = reactive<AppSettings>(settingsSnapshot(appSettings));
const largeFileThresholdError = computed(() => (
  Number.isInteger(form.largeFileThresholdMb)
  && form.largeFileThresholdMb >= 1
  && form.largeFileThresholdMb <= 1024
    ? undefined
    : "请输入 1–1024 之间的整数。"
));

const themeOptions = [
  { label: "跟随系统", value: "system" },
  { label: "浅色", value: "light" },
  { label: "深色", value: "dark" },
];

const residencyOptions = [
  {
    label: "极速模式",
    value: "fast",
    description: "关闭窗口后保留 WebView，再次打开响应更快。",
  },
  {
    label: "低内存模式",
    value: "low-memory",
    description: "关闭窗口后释放 WebView，再次打开时重新创建。",
  },
];

const closeBehaviorOptions = computed(() => [
  {
    label: "隐藏窗口并保留标签页",
    value: "keep-tabs",
    description: "关闭窗口时保留当前工作区，再次打开后继续使用。",
    disabled: form.residencyMode !== "fast",
  },
  {
    label: "关闭所有标签页",
    value: "close-tabs",
    description: "关闭窗口前处理未保存修改，并清空当前工作区。",
  },
]);

const associationGroups = associationGroupsForPlatform("windows");
const isWindows = /windows/i.test(navigator.userAgent);

watch(
  () => props.open,
  (open) => {
    if (open) Object.assign(form, settingsSnapshot(appSettings));
  },
);

watch(
  () => form.residencyMode,
  (mode) => {
    if (mode === "low-memory") form.closeBehavior = "close-tabs";
  },
);

async function submit() {
  if (largeFileThresholdError.value) return;
  saving.value = true;
  try {
    await appSettings.save({ ...form });
    toast.add({ title: "设置已保存", color: "success", icon: "i-lucide-check" });
    emit("update:open", false);
  } catch (error) {
    toast.add({
      title: "设置保存失败",
      description: error instanceof Error ? error.message : String(error),
      color: "error",
    });
  } finally {
    saving.value = false;
  }
}

async function openFileAssociationSettings() {
  if (!isTauri()) {
    toast.add({ title: "文件关联设置仅在 Windows 桌面版中可用", color: "warning" });
    return;
  }
  openingFileAssociations.value = true;
  try {
    await invoke("open_file_association_settings");
  } catch (error) {
    toast.add({
      title: "无法打开文件关联设置",
      description: error instanceof Error ? error.message : String(error),
      color: "error",
    });
  } finally {
    openingFileAssociations.value = false;
  }
}

async function openLogDirectory() {
  if (!isTauri()) {
    toast.add({ title: "日志目录仅在桌面版中可用", color: "warning" });
    return;
  }
  openingLogDirectory.value = true;
  try {
    await invoke("open_log_directory");
  } catch (error) {
    toast.add({
      title: "无法打开日志目录",
      description: error instanceof Error ? error.message : String(error),
      color: "error",
    });
  } finally {
    openingLogDirectory.value = false;
  }
}
</script>

<template>
  <UModal :open="open" title="设置" @update:open="emit('update:open', $event)">
    <template #body>
      <div class="space-y-6">
        <UFormField name="theme" label="外观主题">
          <USelect v-model="form.theme" class="w-full" :items="themeOptions" value-key="value" />
        </UFormField>

        <div class="space-y-4 rounded-xl border border-muted p-4">
          <p class="text-sm font-medium text-highlighted">文本查看</p>
          <UFormField
            name="largeFileThresholdMb"
            label="大文件阈值（MiB）"
            :error="largeFileThresholdError"
          >
            <UInputNumber
              v-model="form.largeFileThresholdMb"
              class="w-full"
              :min="1"
              :max="1024"
              :step="1"
            />
            <p class="mt-2 text-xs leading-5 text-muted">
              阈值以内的本地文本会完整载入内存并使用 Monaco 编辑器；设置过高可能显著增加内存占用。
            </p>
          </UFormField>
        </div>

        <div class="space-y-4 rounded-xl border border-muted p-4">
          <UFormField name="residencyMode" label="驻留模式">
            <URadioGroup v-model="form.residencyMode" :items="residencyOptions" />
          </UFormField>
          <UFormField name="closeBehavior" label="关闭窗口时">
            <URadioGroup v-model="form.closeBehavior" :items="closeBehaviorOptions" />
            <p v-if="form.residencyMode === 'low-memory'" class="mt-2 text-xs leading-5 text-muted">
              低内存模式会销毁窗口，因此必须关闭所有标签页。
            </p>
          </UFormField>
          <UFormField name="autostart" label="开机启动">
            <div class="flex items-start justify-between gap-6">
              <div>
                <p class="mt-1 text-xs leading-5 text-muted">登录系统后静默启动到托盘，不主动显示窗口。</p>
              </div>
              <USwitch v-model="form.autostart" />
            </div>
          </UFormField>
        </div>

        <div v-if="isWindows" class="flex items-center justify-between gap-6 rounded-xl border border-muted p-4">
          <div class="min-w-0">
            <p class="text-sm font-medium text-highlighted">文件关联</p>
            <p class="mt-1 text-xs leading-5 text-muted">
              前往 Windows 默认应用设置，按需选择由 KView 打开的文件格式。
            </p>
            <details class="mt-2 text-xs text-muted">
              <summary class="cursor-pointer select-none text-primary">查看已注册格式</summary>
              <div class="mt-2 space-y-1.5 leading-5">
                <p v-for="group in associationGroups" :key="group.label">
                  <span class="font-medium text-default">{{ group.label }}：</span>
                  {{ group.extensions.map((extension) => `.${extension}`).join("、") }}
                </p>
              </div>
            </details>
          </div>
          <UButton
            class="shrink-0"
            color="neutral"
            variant="soft"
            icon="i-lucide-external-link"
            label="前往设置"
            :loading="openingFileAssociations"
            @click="openFileAssociationSettings"
          />
        </div>

        <div class="flex items-center justify-between gap-6 rounded-xl border border-muted p-4">
          <div class="min-w-0">
            <p class="text-sm font-medium text-highlighted">本地诊断</p>
            <p class="mt-1 text-xs leading-5 text-muted">仅记录错误类别、错误代码和耗时，不记录文件路径或文件内容。</p>
          </div>
          <UButton
            class="shrink-0"
            color="neutral"
            variant="soft"
            icon="i-lucide-folder-open"
            label="打开日志目录"
            :loading="openingLogDirectory"
            @click="openLogDirectory"
          />
        </div>
      </div>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton color="neutral" variant="ghost" label="取消" @click="emit('update:open', false)" />
        <UButton label="保存设置" :loading="saving" :disabled="!!largeFileThresholdError" @click="submit" />
      </div>
    </template>
  </UModal>
</template>
