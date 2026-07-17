import type { OpenedFile, ViewerDefinition } from "../types/document";
import { textExtensions } from "./languages";

const definitions = new Map<string, ViewerDefinition>();
const definitionsByFileName = new Map<string, ViewerDefinition>();
let initialized = false;

export function registerViewer(definition: ViewerDefinition) {
  for (const extension of definition.extensions) {
    definitions.set(extension.toLowerCase(), definition);
  }
  for (const fileName of definition.fileNames ?? []) {
    definitionsByFileName.set(fileName.toLowerCase(), definition);
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
    extensions: textExtensions,
    fileNames: ["Dockerfile", "Makefile", "CMakeLists.txt"],
    readOnly: false,
    maxEditableBytes: 20 * 1024 * 1024,
    load: () => import("./TextViewer.vue"),
  });
}

export function resolveViewer(file: Pick<OpenedFile, "extension">) {
  initializeViewerRegistry();
  const openedFile = file as Pick<OpenedFile, "extension"> & Partial<Pick<OpenedFile, "name">>;
  return definitions.get(file.extension.toLowerCase())
    ?? definitionsByFileName.get(openedFile.name?.toLowerCase() ?? "")
    ?? null;
}

export function clearViewerRegistry() {
  definitions.clear();
  definitionsByFileName.clear();
  initialized = false;
}
