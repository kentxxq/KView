<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import { useToast } from "@nuxt/ui/composables";
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
        <UFormField name="theme" label="外观主题">
          <USelect v-model="form.theme" class="w-full" :items="themeOptions" value-key="value" />
        </UFormField>

        <div class="space-y-4 rounded-xl border border-muted p-4">
          <UFormField name="residencyMode" label="驻留模式">
            <URadioGroup v-model="form.residencyMode" :items="residencyOptions" />
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
