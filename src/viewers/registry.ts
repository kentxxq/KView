import type { OpenedFile, ViewerDefinition } from "../types/document";

const definitions = new Map<string, ViewerDefinition>();

export function registerViewer(definition: ViewerDefinition) {
  for (const extension of definition.extensions) {
    definitions.set(extension.toLowerCase(), definition);
  }
}

export function resolveViewer(file: Pick<OpenedFile, "extension">) {
  return definitions.get(file.extension.toLowerCase()) ?? null;
}

export function clearViewerRegistry() {
  definitions.clear();
}
