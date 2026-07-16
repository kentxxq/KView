import { reactive } from "vue";
import type { DocumentState } from "../types/document";

const initialState = (): DocumentState => ({
  file: null,
  status: "empty",
  viewerKind: "empty",
  buffer: null,
  dirty: false,
  sourceModifiedAt: null,
  error: null,
});

export const documentState = reactive<DocumentState>(initialState());

export function resetDocument() {
  Object.assign(documentState, initialState());
}
