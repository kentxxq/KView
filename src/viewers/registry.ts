import type { OpenedFile, ViewerDefinition } from "../types/document";
import { capabilitiesForPlatform, currentDesktopPlatform, formatCapabilities } from "../domain/formats";

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

  const loaders = {
    image: () => import("./ImageViewer.vue"),
    "live-photo": () => import("./ImageViewer.vue"),
    pdf: () => import("./PdfViewer.vue"),
    docx: () => import("./DocxViewer.vue"),
    xlsx: () => import("./XlsxViewer.vue"),
    ebook: () => import("./EbookViewer.vue"),
    archive: () => import("./ArchiveViewer.vue"),
    video: () => import("./VideoViewer.vue"),
    audio: () => import("./AudioViewer.vue"),
    installer: () => import("./InstallerViewer.vue"),
    markdown: () => import("./MarkdownViewer.vue"),
    json: () => import("./StructuredViewer.vue"),
    yaml: () => import("./StructuredViewer.vue"),
    text: () => import("./TextViewer.vue"),
  } satisfies Partial<Record<OpenedFile["kind"], ViewerDefinition["load"]>>;

  const platform = currentDesktopPlatform();
  const capabilities = platform ? capabilitiesForPlatform(platform) : formatCapabilities;
  for (const capability of capabilities) {
    const load = loaders[capability.viewerKind as keyof typeof loaders];
    if (!load) continue;
    registerViewer({
      kind: capability.viewerKind,
      componentKey: capability.viewerKind === "live-photo"
        ? "image"
        : ["json", "yaml"].includes(capability.viewerKind)
          ? "structured"
          : undefined,
      extensions: capability.extensions,
      fileNames: capability.fileNames,
      readOnly: !capability.editable,
      load,
    });
  }
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
