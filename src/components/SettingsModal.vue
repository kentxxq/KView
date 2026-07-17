<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import { appSettings, saveSettings, type AppSettings } from "../state/settings";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();
const toast = useToast();
const saving = ref(false);
const form = reactive<AppSettings>({ ...appSettings });

const themeOptions = [
  { label: "跟随系统", value: "system" },
  { label: "浅色", value: "light" },
  { label: "深色", value: "dark" },
];

watch(
  () => props.open,
  (open) => {
    if (open) Object.assign(form, appSettings);
  },
);

async function submit() {
  saving.value = true;
  try {
    await saveSettings({ ...form });
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
</script>

<template>
  <UModal :open="open" title="设置" @update:open="emit('update:open', $event)">
    <template #body>
      <div class="space-y-6">
        <div>
          <label class="mb-2 block text-sm font-medium text-highlighted">外观主题</label>
          <USelect v-model="form.theme" class="w-full" :items="themeOptions" value-key="value" />
        </div>

        <div class="space-y-4 rounded-xl border border-muted p-4">
          <div class="flex items-start justify-between gap-6">
            <div>
              <p class="text-sm font-medium text-highlighted">关闭窗口后在后台运行</p>
              <p class="mt-1 text-xs leading-5 text-muted">关闭窗口后保留托盘；关闭此项时，关闭窗口会退出 KView。</p>
            </div>
            <USwitch v-model="form.keepRunning" />
          </div>
          <div class="flex items-start justify-between gap-6">
            <div>
              <p class="text-sm font-medium text-highlighted">开机启动</p>
              <p class="mt-1 text-xs leading-5 text-muted">登录系统后静默启动到托盘，不主动显示窗口。</p>
            </div>
            <USwitch v-model="form.autostart" />
          </div>
        </div>

        <UAlert
          color="neutral"
          variant="subtle"
          icon="i-lucide-shield-check"
          title="本地诊断"
          description="仅记录错误类别、错误代码和耗时，不记录文件路径或文件内容。日志保存在应用本地目录。"
        />
      </div>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton color="neutral" variant="ghost" label="取消" @click="emit('update:open', false)" />
        <UButton label="保存设置" :loading="saving" @click="submit" />
      </div>
    </template>
  </UModal>
</template>
