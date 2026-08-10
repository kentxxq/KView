import { reactive } from "vue";
import type { DocumentState } from "../types/document";

export const createDocumentState = (): DocumentState => ({
  file: null,
  status: "empty",
  viewerKind: "empty",
  content: { type: "empty" },
  dirty: false,
  sourceModifiedAt: null,
  encoding: null,
  lineEnding: null,
  bom: false,
  readOnly: true,
  largeTextSession: null,
  error: null,
});

export const documentState = reactive<DocumentState>(createDocumentState());

export function resetDocument() {
  Object.assign(documentState, createDocumentState());
}
