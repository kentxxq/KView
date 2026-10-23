import type { DocumentState } from "../types/document";
import { desktopInvoke } from "./desktop-api";
import { recordDiagnostic } from "./diagnostics";
import { disposeTextModel } from "./text-models";

const loadGenerations = new WeakMap<DocumentState, number>();
interface ResourceHandles {
  objectUrls: Set<string>;
  largeTextSessionId?: string;
  ebookSessionId?: string;
  logSessionId?: string;
}
const resources = new WeakMap<DocumentState, ResourceHandles>();

function handlesFor(state: DocumentState) {
  let handles = resources.get(state);
  if (!handles) {
    handles = { objectUrls: new Set() };
    resources.set(state, handles);
  }
  return handles;
}

export function beginDocumentLoad(state: DocumentState) {
  const generation = (loadGenerations.get(state) ?? 0) + 1;
  loadGenerations.set(state, generation);
  return generation;
}

export function isCurrentDocumentLoad(state: DocumentState, generation: number) {
  return loadGenerations.get(state) === generation;
}

export function createDocumentObjectUrl(
  state: DocumentState,
  bytes: Uint8Array<ArrayBuffer>,
  mimeType: string,
) {
  const url = URL.createObjectURL(new Blob([bytes], { type: mimeType }));
  handlesFor(state).objectUrls.add(url);
  return url;
}

export function registerLargeTextSession(state: DocumentState, sessionId: string) {
  handlesFor(state).largeTextSessionId = sessionId;
}

export function registerEbookSession(state: DocumentState, sessionId: string) {
  handlesFor(state).ebookSessionId = sessionId;
}

export function registerLogSession(state: DocumentState, sessionId: string) {
  handlesFor(state).logSessionId = sessionId;
}

export function unregisterLogSession(state: DocumentState, sessionId: string) {
  const handles = resources.get(state);
  if (handles?.logSessionId === sessionId) delete handles.logSessionId;
}

async function releaseHandles(state: DocumentState) {
  disposeTextModel(state);
  const handles = resources.get(state);
  if (!handles) return;
  resources.delete(state);
  for (const url of handles.objectUrls) URL.revokeObjectURL(url);
  const largeTextSessionId = handles.largeTextSessionId;
  const ebookSessionId = handles.ebookSessionId;
  state.largeTextSession = null;
  await Promise.all([
    handles.logSessionId
      ? desktopInvoke("close_log_session", { sessionId: handles.logSessionId }).catch(() => recordDiagnostic("viewer", "log-close-failed"))
      : Promise.resolve(),
    largeTextSessionId
      ? desktopInvoke("close_large_text_session", { sessionId: largeTextSessionId }).catch(() =>
          recordDiagnostic("viewer", "large-text-close-failed"),
        )
      : Promise.resolve(),
    ebookSessionId
      ? desktopInvoke("close_epub", { sessionId: ebookSessionId }).catch(() =>
          recordDiagnostic("viewer", "epub-close-failed"),
        )
      : Promise.resolve(),
  ]);
}

export async function disposeDocumentResources(state: DocumentState) {
  loadGenerations.set(state, (loadGenerations.get(state) ?? 0) + 1);
  await releaseHandles(state);
}

export function transferDocumentResources(from: DocumentState, to: DocumentState) {
  const handles = resources.get(from);
  if (!handles) return;
  resources.delete(from);
  resources.set(to, handles);
}

export async function commitDocumentLoad(
  target: DocumentState,
  loaded: DocumentState,
  generation: number,
) {
  if (!isCurrentDocumentLoad(target, generation)) {
    await releaseHandles(loaded);
    return false;
  }
  await releaseHandles(target);
  if (!isCurrentDocumentLoad(target, generation)) {
    await releaseHandles(loaded);
    return false;
  }
  transferDocumentResources(loaded, target);
  Object.assign(target, loaded);
  return true;
}
