<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { useToast } from "@nuxt/ui/composables";
import { getVersion } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import packageJson from "../../package.json";
import appLogoUrl from "../../src-tauri/icons/icon.png";
import { desktopInvoke as invoke } from "../services/desktop-api";
import { useSettingsStore, type AppSettings } from "../state/settings";
import { associationGroupsForPlatform } from "../domain/formats";
import UpdatePanel from "./UpdatePanel.vue";
import { errorMessage } from "../utils/error";

const props = defineProps<{ open: boolean; initialSection?: "general" | "about" }>();
const emit = defineEmits<{ "update:open": [value: boolean]; "request-update": [] }>();
const toast = useToast();
const appSettings = useSettingsStore();
const saving = ref(false);
const openingFileAssociations = ref(false);
const associationAction = ref<"register" | "unregister" | null>(null);
const associationBusy = computed(() => openingFileAssociations.value || associationAction.value !== null);
const openingLogDirectory = ref(false);
type SettingsSection = "general" | "text" | "associations" | "diagnostics" | "about";

const activeSection = ref<SettingsSection>("general");
defineExpose({
  showAbout() {
    activeSection.value = "about";
  },
});
watch(() => props.initialSection, (section) => {
  if (props.open && section) activeSection.value = section;
});
const appVersion = ref(packageJson.version);
const isWindows = /windows/i.test(navigator.userAgent);
const autostartAvailable = !import.meta.env.DEV;

const settingsSections = computed(() => [
  { label: "常规", value: "general" as const, icon: "i-lucide-settings-2" },
  { label: "文本查看", value: "text" as const, icon: "i-lucide-file-text" },
  ...(isWindows
    ? [{ label: "文件关联", value: "associations" as const, icon: "i-lucide-files" }]
    : []),
  { label: "诊断", value: "diagnostics" as const, icon: "i-lucide-stethoscope" },
  { label: "关于", value: "about" as const, icon: "i-lucide-info" },
]);
function settingsSnapshot(settings: AppSettings): AppSettings {
  return {
    theme: settings.theme,
    residencyMode: settings.residencyMode,
    closeBehavior: settings.closeBehavior,
    autostart: settings.autostart,
    largeFileThresholdMb: settings.largeFileThresholdMb,
    logHighlightRules: settings.logHighlightRules.map((rule) => ({ ...rule })),
  };
}

const form = reactive<AppSettings>(settingsSnapshot(appSettings));
const largeFileThresholdError = computed(() =>
  Number.isInteger(form.largeFileThresholdMb) &&
  form.largeFileThresholdMb >= 1 &&
  form.largeFileThresholdMb <= 1024
    ? undefined
    : "请输入 1–1024 之间的整数。",
);

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

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    Object.assign(form, settingsSnapshot(appSettings));
    activeSection.value = props.initialSection ?? "general";
    if (isTauri()) {
      void getVersion()
        .then((version) => {
          appVersion.value = version;
        })
        .catch(() => undefined);
    }
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
      description: errorMessage(error),
      color: "error",
    });
  } finally {
    saving.value = false;
  }
}

async function openFileAssociationSettings() {
  if (associationBusy.value) return;
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
      description: errorMessage(error),
      color: "error",
    });
  } finally {
    openingFileAssociations.value = false;
  }
}

async function changeFileAssociations(action: "register" | "unregister") {
  if (associationBusy.value) return;
  if (!isTauri()) {
    toast.add({ title: "文件关联注册仅在 Windows 桌面版中可用", color: "warning" });
    return;
  }
  associationAction.value = action;
  try {
    await invoke(action === "register" ? "register_file_associations" : "unregister_file_associations");
    toast.add({
      title: action === "register" ? "文件关联已注册" : "文件关联注册已取消",
      description: action === "register"
        ? "可前往 Windows 默认应用设置，选择由 KView 打开的格式。"
        : "若此前已将 KView 设为默认应用，请前往 Windows 设置选择其他应用。",
      color: "success",
    });
  } catch (error) {
    toast.add({
      title: action === "register" ? "文件关联注册失败，可重试" : "取消注册未完成，可重试",
      description: errorMessage(error),
      color: "error",
    });
  } finally {
    associationAction.value = null;
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
      description: errorMessage(error),
      color: "error",
    });
  } finally {
    openingLogDirectory.value = false;
  }
}
</script>

<template>
  <UModal
    :open="open"
    title="设置"
    :ui="{
      content: 'h-[min(46rem,calc(100dvh-4rem))] sm:max-w-6xl',
      header: 'shrink-0',
      body: 'flex min-h-0 flex-1 overflow-hidden p-0 sm:p-0',
    }"
    @update:open="emit('update:open', $event)"
  >
    <template #body>
      <div class="flex min-h-0 w-full flex-1">
        <nav class="w-44 shrink-0 border-r border-default bg-muted/40 p-3" aria-label="设置分类">
          <button
            v-for="section in settingsSections"
            :key="section.value"
            type="button"
            class="mb-1 flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors"
            :class="
              activeSection === section.value
                ? 'bg-primary/10 font-medium text-primary'
                : 'text-muted hover:bg-elevated hover:text-highlighted'
            "
            :aria-current="activeSection === section.value ? 'page' : undefined"
            @click="activeSection = section.value"
          >
            <UIcon :name="section.icon" class="size-4 shrink-0" />
            <span>{{ section.label }}</span>
          </button>
        </nav>

        <div class="flex min-w-0 flex-1 flex-col">
          <div class="min-h-0 flex-1 overflow-y-auto overscroll-contain p-6">
            <section v-if="activeSection === 'general'" class="space-y-6">
              <div>
                <h2 class="text-base font-semibold text-highlighted">常规</h2>
                <p class="mt-1 text-sm text-muted">调整应用外观和窗口行为。</p>
              </div>
              <UFormField name="theme" label="外观主题">
                <USelect
                  v-model="form.theme"
                  class="w-full"
                  :items="themeOptions"
                  value-key="value"
                />
              </UFormField>
              <div class="space-y-5 rounded-xl border border-muted p-4">
                <UFormField name="residencyMode" label="驻留模式">
                  <URadioGroup v-model="form.residencyMode" :items="residencyOptions" />
                </UFormField>
                <UFormField name="closeBehavior" label="关闭窗口时">
                  <URadioGroup v-model="form.closeBehavior" :items="closeBehaviorOptions" />
                  <p
                    v-if="form.residencyMode === 'low-memory'"
                    class="mt-2 text-xs leading-5 text-muted"
                  >
                    低内存模式会销毁窗口，因此必须关闭所有标签页。
                  </p>
                </UFormField>
                <UFormField name="autostart" label="开机启动">
                  <div class="flex items-start justify-between gap-6">
                    <p class="mt-1 text-xs leading-5 text-muted">
                      {{
                        autostartAvailable
                          ? "登录系统后静默启动到托盘，不主动显示窗口。"
                          : "开发模式不管理系统开机启动。"
                      }}
                    </p>
                    <USwitch v-model="form.autostart" :disabled="!autostartAvailable" />
                  </div>
                </UFormField>
              </div>
            </section>

            <section v-else-if="activeSection === 'text'" class="space-y-6">
              <div>
                <h2 class="text-base font-semibold text-highlighted">文本查看</h2>
                <p class="mt-1 text-sm text-muted">配置本地文本文件的载入方式。</p>
              </div>
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
                  阈值以内的本地文本会完整载入内存并使用 Monaco
                  编辑器；设置过高可能显著增加内存占用。
                </p>
              </UFormField>
            </section>

            <section v-else-if="activeSection === 'associations'" class="space-y-6">
              <div>
                <h2 class="text-base font-semibold text-highlighted">文件关联</h2>
                <p class="mt-1 text-sm text-muted">按需将 KView 添加到系统的文件打开方式中。</p>
              </div>
              <div
                class="space-y-4 rounded-xl border border-muted p-4"
              >
                <div class="min-w-0">
                  <p class="text-sm text-muted">
                    安装时不会自动注册。点击“注册文件关联”后，KView 会出现在支持格式的打开方式中，默认应用仍由你在 Windows 设置中选择。
                  </p>
                  <p class="mt-2 text-sm text-muted">
                    “取消注册”会清理 KView 的注册记录；若此前已设为默认应用，请前往系统设置选择其他应用。这些操作立即生效。
                  </p>
                  <details class="mt-3 text-xs text-muted">
                    <summary class="cursor-pointer select-none text-primary">
                      查看支持注册的格式
                    </summary>
                    <div class="mt-2 space-y-1.5 leading-5">
                      <p v-for="group in associationGroups" :key="group.label">
                        <span class="font-medium text-default">{{ group.label }}：</span>
                        {{ group.extensions.map((extension) => `.${extension}`).join("、") }}
                      </p>
                    </div>
                  </details>
                </div>
                <div class="flex flex-wrap gap-2">
                  <UButton
                    label="注册文件关联"
                    icon="i-lucide-file-plus"
                    :loading="associationAction === 'register'"
                    :disabled="associationBusy"
                    @click="changeFileAssociations('register')"
                  />
                  <UButton
                    label="取消注册"
                    color="neutral"
                    variant="soft"
                    icon="i-lucide-file-minus"
                    :loading="associationAction === 'unregister'"
                    :disabled="associationBusy"
                    @click="changeFileAssociations('unregister')"
                  />
                  <UButton
                    class="shrink-0"
                    color="neutral"
                    variant="soft"
                    icon="i-lucide-external-link"
                    label="前往系统设置"
                    :loading="openingFileAssociations"
                    :disabled="associationBusy"
                    @click="openFileAssociationSettings"
                  />
                </div>
              </div>
            </section>

            <section v-else-if="activeSection === 'diagnostics'" class="space-y-6">
              <div>
                <h2 class="text-base font-semibold text-highlighted">诊断</h2>
                <p class="mt-1 text-sm text-muted">查看用于排查问题的本地日志。</p>
              </div>
              <div
                class="flex items-center justify-between gap-6 rounded-xl border border-muted p-4"
              >
                <p class="text-sm leading-6 text-muted">
                  仅记录错误类别、错误代码和耗时，不记录文件路径或文件内容。
                </p>
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
            </section>

            <section v-else class="mx-auto flex min-h-full w-full max-w-2xl flex-col justify-center gap-6">
              <div class="flex shrink-0 items-center justify-center gap-5">
                <img :src="appLogoUrl" alt="KView logo" class="size-20 shrink-0" />
                <div>
                  <h2 class="text-2xl font-semibold text-highlighted">KView</h2>
                  <p class="mt-2 text-sm text-muted">版本 {{ appVersion }}</p>
                </div>
              </div>
              <UpdatePanel @request-update="emit('request-update')" />
            </section>
          </div>

          <div
            v-if="activeSection === 'general' || activeSection === 'text'"
            class="flex shrink-0 justify-end gap-2 border-t border-default px-6 py-3"
          >
            <UButton
              color="neutral"
              variant="ghost"
              label="取消"
              @click="emit('update:open', false)"
            />
            <UButton
              label="保存设置"
              :loading="saving"
              :disabled="!!largeFileThresholdError"
              @click="submit"
            />
          </div>
        </div>
      </div>
    </template>
  </UModal>
</template>
