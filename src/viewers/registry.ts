import type { OpenedFile, ViewerDefinition } from "../types/document";

const definitions = new Map<string, ViewerDefinition>();
let initialized = false;

export function registerViewer(definition: ViewerDefinition) {
  for (const extension of definition.extensions) {
    definitions.set(extension.toLowerCase(), definition);
  }
}

export function initializeViewerRegistry() {
  if (initialized) return;
  initialized = true;

  registerViewer({
    kind: "image",
    extensions: ["jpg", "jpeg", "png", "webp", "gif", "svg"],
    readOnly: true,
    maxEditableBytes: null,
    load: () => import("./ImageViewer.vue"),
  });
  registerViewer({
    kind: "markdown",
    extensions: ["md", "markdown"],
    readOnly: false,
    maxEditableBytes: 20 * 1024 * 1024,
    load: () => import("./MarkdownViewer.vue"),
  });
  registerViewer({
    kind: "json",
    extensions: ["json"],
    readOnly: false,
    maxEditableBytes: 20 * 1024 * 1024,
    load: () => import("./StructuredViewer.vue"),
  });
  registerViewer({
    kind: "yaml",
    extensions: ["yaml", "yml"],
    readOnly: false,
    maxEditableBytes: 20 * 1024 * 1024,
    load: () => import("./StructuredViewer.vue"),
  });
  registerViewer({
    kind: "text",
    extensions: ["txt", "log", "xml", "toml", "ini", "env", "conf", "sql"],
    readOnly: false,
    maxEditableBytes: 20 * 1024 * 1024,
    load: () => import("./TextViewer.vue"),
  });
}

export function resolveViewer(file: Pick<OpenedFile, "extension">) {
  initializeViewerRegistry();
  return definitions.get(file.extension.toLowerCase()) ?? null;
}

export function clearViewerRegistry() {
  definitions.clear();
  initialized = false;
}
