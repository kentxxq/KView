<script setup lang="ts">
import AppShell from "./components/AppShell.vue";
import EmptyState from "./components/EmptyState.vue";
import ErrorState from "./components/ErrorState.vue";
import { documentState, resetDocument } from "./state/document";

function acknowledgePendingOpen() {
  documentState.error = {
    code: "open-not-ready",
    title: "文件打开功能即将可用",
    message: "当前工程基线已经就绪，文件选择与拖拽打开将在下一个里程碑接入。",
    recoverable: true,
  };
}
</script>

<template>
  <UApp>
    <AppShell>
      <ErrorState
        v-if="documentState.error"
        :error="documentState.error"
        @dismiss="resetDocument"
      />
      <EmptyState v-else @select="acknowledgePendingOpen" />
    </AppShell>
  </UApp>
</template>
