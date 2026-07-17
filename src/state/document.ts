import { reactive } from "vue";
import type { DocumentState } from "../types/document";

export const createDocumentState = (): DocumentState => ({
  file: null,
  status: "empty",
  viewerKind: "empty",
  buffer: null,
  dirty: false,
  sourceModifiedAt: null,
  encoding: null,
  lineEnding: null,
  bom: false,
  readOnly: true,
  truncated: false,
  error: null,
});

export const documentState = reactive<DocumentState>(createDocumentState());

export function resetDocument() {
  Object.assign(documentState, createDocumentState());
}
