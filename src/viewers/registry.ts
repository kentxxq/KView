import type { OpenedFile, ViewerDefinition } from "../types/document";

const definitions = new Map<string, ViewerDefinition>();
const definitionsByFileName = new Map<string, ViewerDefinition>();
const definitionsByKind = new Map<OpenedFile["kind"], ViewerDefinition>();
let initialized = false;

export function registerViewer(definition: ViewerDefinition) {
  definitionsByKind.set(definition.kind, definition);
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
    kind: "pdf",
    extensions: ["pdf"],
    readOnly: true,
    maxEditableBytes: null,
    load: () => import("./PdfViewer.vue"),
  });
  registerViewer({
    kind: "docx",
    extensions: ["doc", "docx"],
    readOnly: true,
    maxEditableBytes: null,
    load: () => import("./DocxViewer.vue"),
  });
  registerViewer({
    kind: "xlsx",
    extensions: ["xls", "xlsx"],
    readOnly: true,
    maxEditableBytes: null,
    load: () => import("./XlsxViewer.vue"),
  });
  registerViewer({
    kind: "archive",
    extensions: ["zip", "tar", "tgz", "gz"],
    readOnly: true,
    maxEditableBytes: null,
    load: () => import("./ArchiveViewer.vue"),
  });
  registerViewer({
    kind: "video",
    extensions: ["mp4", "webm", "mov"],
    readOnly: true,
    maxEditableBytes: null,
    load: () => import("./VideoViewer.vue"),
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
    extensions: [],
    fileNames: ["Dockerfile", "Makefile", "CMakeLists.txt"],
    readOnly: false,
    maxEditableBytes: 20 * 1024 * 1024,
    load: () => import("./TextViewer.vue"),
  });
  registerViewer({
    kind: "hex",
    extensions: [],
    readOnly: true,
    maxEditableBytes: null,
    load: () => import("./HexViewer.vue"),
  });
}

export function resolveViewer(file: Pick<OpenedFile, "extension" | "kind">) {
  initializeViewerRegistry();
  const openedFile = file as Pick<OpenedFile, "extension" | "kind"> & Partial<Pick<OpenedFile, "name">>;
  return definitionsByKind.get(file.kind)
    ?? definitions.get(file.extension.toLowerCase())
    ?? definitionsByFileName.get(openedFile.name?.toLowerCase() ?? "")
    ?? null;
}

export function clearViewerRegistry() {
  definitions.clear();
  definitionsByFileName.clear();
  definitionsByKind.clear();
  initialized = false;
}
