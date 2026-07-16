<template>
  <div class="flex min-h-screen flex-col bg-default text-default">
    <header
      class="flex h-14 shrink-0 items-center justify-between border-b border-muted px-5"
    >
      <div class="flex items-center gap-2.5">
        <div
          class="flex size-8 items-center justify-center rounded-lg bg-primary text-inverted shadow-sm"
        >
          <UIcon name="i-lucide-scan-eye" class="size-4.5" />
        </div>
        <div>
          <p class="text-sm font-semibold leading-tight text-highlighted">KView</p>
          <p class="text-[11px] leading-tight text-muted">快速文件查看器</p>
        </div>
      </div>

      <slot name="actions" />
    </header>

    <main class="flex min-h-0 flex-1 items-center justify-center p-6 sm:p-10">
      <slot />
    </main>

    <footer
      class="flex h-9 shrink-0 items-center justify-between border-t border-muted px-5 text-xs text-muted"
    >
      <span>准备就绪</span>
      <span>文件仅在本机处理</span>
    </footer>
  </div>
</template>
