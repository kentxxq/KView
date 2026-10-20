import { save } from "@tauri-apps/plugin-dialog";
import type { OpenedFile } from "../types/document";
import { desktopInvoke } from "./desktop-api";

type PdfBytes = Uint8Array<ArrayBuffer>;
export interface PdfAnnotationSession {
  exportBytes: () => Promise<PdfBytes>;
}

// 会话以原始二进制对象为键，允许 Viewer 缓存淘汰后恢复未保存的标注。
const sessions = new WeakMap<PdfBytes, PdfAnnotationSession>();

export function getPdfAnnotationSession(bytes: PdfBytes): PdfAnnotationSession {
  let session = sessions.get(bytes);
  if (!session) {
    session = { exportBytes: async () => bytes.slice() };
    sessions.set(bytes, session);
  }
  return session;
}

export async function saveAnnotatedPdf(file: OpenedFile, bytes: PdfBytes): Promise<boolean> {
  const target = await save({
    defaultPath: file.name.replace(/\.pdf$/i, "") + "-标注.pdf",
    filters: [{ name: "PDF 文档", extensions: ["pdf"] }],
  });
  if (!target) return false;
  const data = await getPdfAnnotationSession(bytes).exportBytes();
  await desktopInvoke("save_annotated_pdf", {
    path: target,
    sourcePath: file.source === "local" ? file.path : null,
    bytes: Array.from(data),
  });
  return true;
}
