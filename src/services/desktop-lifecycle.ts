import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface DesktopLifecycleHandlers {
  onPaths: (paths: string[]) => void | Promise<void>;
  onRequestClose: () => boolean | Promise<boolean>;
  onBeforeClose: () => void | Promise<void>;
  onDragStateChange: (active: boolean) => void;
}

export async function initializeDesktopLifecycle(handlers: DesktopLifecycleHandlers) {
  const unlisteners: UnlistenFn[] = [];

  const consumePendingPaths = async () => {
    const paths = await invoke<string[]>("take_pending_paths");
    if (paths.length) await handlers.onPaths(paths);
  };

  unlisteners.push(await listen("open-requested", consumePendingPaths));
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
      if (await handlers.onRequestClose()) {
        await handlers.onBeforeClose();
        await invoke("close_main_window");
      }
    }),
  );

  await consumePendingPaths();

  return () => {
    for (const unlisten of unlisteners) unlisten();
  };
}
