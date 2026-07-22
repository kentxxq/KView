import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface DesktopLifecycleHandlers {
  onPaths: (paths: string[]) => void | Promise<void>;
  onSpaceOpenPath?: (path: string | null) => void | Promise<void>;
  onSpaceOpenClosed?: () => void;
  isSpaceOpenActive?: () => boolean;
  onRequestClose: () => boolean | Promise<boolean>;
  onBeforeClose: () => void | Promise<void>;
  onDragStateChange: (active: boolean) => void;
  onOpenFile?: () => void | Promise<void>;
  onCloseTab?: () => void | Promise<void>;
  onNextTab?: () => void;
  onPreviousTab?: () => void;
}

export async function initializeDesktopLifecycle(handlers: DesktopLifecycleHandlers) {
  const unlisteners: UnlistenFn[] = [];

  const consumePendingPaths = async () => {
    const paths = await invoke<string[]>("take_pending_paths");
    if (paths.length) await handlers.onPaths(paths);
  };

  const consumePendingSpaceOpen = async () => {
    const request = await invoke<{ path: string | null } | null>("take_pending_space_open");
    if (request && handlers.onSpaceOpenPath) await handlers.onSpaceOpenPath(request.path);
  };

  unlisteners.push(await listen("open-requested", consumePendingPaths));
  unlisteners.push(await listen("space-open-path-changed", consumePendingSpaceOpen));
  unlisteners.push(await listen("space-open-closed", () => handlers.onSpaceOpenClosed?.()));
  if (handlers.onOpenFile) unlisteners.push(await listen("menu-open-file", handlers.onOpenFile));
  if (handlers.onCloseTab) unlisteners.push(await listen("menu-close-tab", handlers.onCloseTab));
  if (handlers.onNextTab) unlisteners.push(await listen("menu-next-tab", handlers.onNextTab));
  if (handlers.onPreviousTab) unlisteners.push(await listen("menu-previous-tab", handlers.onPreviousTab));
  unlisteners.push(
    await getCurrentWebview().onDragDropEvent(async (event) => {
      if (event.payload.type === "over") {
        handlers.onDragStateChange(true);
      } else if (event.payload.type === "drop") {
        handlers.onDragStateChange(false);
        await handlers.onPaths(event.payload.paths);
      } else {
        handlers.onDragStateChange(false);
      }
    }),
  );
  unlisteners.push(
    await getCurrentWindow().onCloseRequested(async (event) => {
      event.preventDefault();
      if (handlers.isSpaceOpenActive?.()) {
        await invoke("close_space_open");
        return;
      }
      if (await handlers.onRequestClose()) {
        await handlers.onBeforeClose();
        await invoke("close_main_window");
      }
    }),
  );

  await consumePendingPaths();
  await consumePendingSpaceOpen();

  return () => {
    for (const unlisten of unlisteners) unlisten();
  };
}
