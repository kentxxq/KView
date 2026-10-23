import { watch, type InjectionKey } from "vue";
import type { editor } from "monaco-editor";
import type { DocumentState } from "../types/document";
import { resolveLanguage } from "../viewers/languages";

export const textDocumentKey: InjectionKey<DocumentState> = Symbol("text-document");
const models = new WeakMap<DocumentState, { model: editor.ITextModel; dispose: () => void }>();
const generations = new WeakMap<DocumentState, number>();

export async function textModelFor(state: DocumentState) {
  const cached = models.get(state);
  if (cached) return cached.model;
  const generation = generations.get(state) ?? 0;
  const monaco = await import("./monaco-runtime");
  const api = await monaco.loadMonaco();
  if ((generations.get(state) ?? 0) !== generation) throw new Error("文本文档已切换或关闭");
  const existing = models.get(state);
  if (existing) return existing.model;
  if (!state.file || state.content.type !== "text") throw new Error("当前文档不是完整文本");
  const model = api.editor.createModel(state.content.value, resolveLanguage(state.file));
  let syncing = false;
  const listener = model.onDidChangeContent(() => {
    if (syncing) return;
    state.content = { type: "text", value: model.getValue() };
    state.dirty = true;
  });
  const stop = watch(() => state.content, (content) => {
    if (content.type !== "text" || content.value === model.getValue()) return;
    syncing = true;
    model.setValue(content.value);
    syncing = false;
  }, { flush: "sync" });
  models.set(state, { model, dispose: () => { stop(); listener.dispose(); model.dispose(); } });
  return model;
}

export function disposeTextModel(state: DocumentState) {
  generations.set(state, (generations.get(state) ?? 0) + 1);
  models.get(state)?.dispose();
  models.delete(state);
}
