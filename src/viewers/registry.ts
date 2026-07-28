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
    extensions: ["jpg", "jpeg", "heic", "heif", "png", "webp", "gif", "svg"],
    readOnly: true,
    load: () => import("./ImageViewer.vue"),
  });
  registerViewer({
    kind: "live-photo",
    componentKey: "image",
    extensions: ["livp"],
    readOnly: true,
    load: () => import("./ImageViewer.vue"),
  });
  registerViewer({
    kind: "pdf",
    extensions: ["pdf"],
    readOnly: true,
    load: () => import("./PdfViewer.vue"),
  });
  registerViewer({
    kind: "docx",
    extensions: ["doc", "docx"],
    readOnly: true,
    load: () => import("./DocxViewer.vue"),
  });
  registerViewer({
    kind: "xlsx",
    extensions: ["xls", "xlsx"],
    readOnly: true,
    load: () => import("./XlsxViewer.vue"),
  });
  registerViewer({
    kind: "archive",
    extensions: ["zip", "tar", "tgz", "gz"],
    readOnly: true,
    load: () => import("./ArchiveViewer.vue"),
  });
  registerViewer({
    kind: "video",
    extensions: ["mp4", "webm", "mov"],
    readOnly: true,
    load: () => import("./VideoViewer.vue"),
  });
  registerViewer({
    kind: "installer",
    extensions: ["exe", "msi", "msixbundle"],
    readOnly: true,
    load: () => import("./InstallerViewer.vue"),
  });
  registerViewer({
    kind: "markdown",
    extensions: ["md", "markdown"],
    readOnly: false,
    load: () => import("./MarkdownViewer.vue"),
  });
  registerViewer({
    kind: "json",
    componentKey: "structured",
    extensions: ["json"],
    readOnly: false,
    load: () => import("./StructuredViewer.vue"),
  });
  registerViewer({
    kind: "yaml",
    componentKey: "structured",
    extensions: ["yaml", "yml"],
    readOnly: false,
    load: () => import("./StructuredViewer.vue"),
  });
  registerViewer({
    kind: "text",
    extensions: [],
    fileNames: ["Dockerfile", "Makefile", "CMakeLists.txt"],
    readOnly: false,
    load: () => import("./TextViewer.vue"),
  });
  registerViewer({
    kind: "hex",
    extensions: [],
    readOnly: true,
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

export function resolveViewerKey(file: Pick<OpenedFile, "extension" | "kind">) {
  const definition = resolveViewer(file);
  return definition?.componentKey ?? definition?.kind ?? null;
}

export function clearViewerRegistry() {
  definitions.clear();
  definitionsByFileName.clear();
  definitionsByKind.clear();
  initialized = false;
}
