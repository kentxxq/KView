import { createDocumentState } from "../state/document";
import type {
  ArchiveDocument,
  DocumentState,
  EpubDocument,
  HtmlDocument,
  ImageDocument,
  InstallerDocument,
  LivePhotoBinaryHeader,
  LivePhotoDocument,
  OpenedFile,
  TextReadResult,
} from "../types/document";
import { resolveViewer } from "../viewers/registry";
import { desktopInvoke, desktopInvokeBytes } from "./desktop-api";
import {
  beginDocumentLoad,
  commitDocumentLoad,
  createDocumentObjectUrl,
  disposeDocumentResources,
  registerEbookSession,
  registerLargeTextSession,
} from "./document-resources";

interface LoadDocumentOptions {
  inspectedFile?: OpenedFile;
  onRecent?: (path: string, name: string) => void;
}

async function loadLivePhoto(state: DocumentState, path: string): Promise<LivePhotoDocument> {
  const payload = await desktopInvokeBytes("read_live_photo", { path });
  if (payload.byteLength < 4) throw new Error("动态照片数据不完整");
  const headerLength = new DataView(
    payload.buffer,
    payload.byteOffset,
    payload.byteLength,
  ).getUint32(0, true);
  const contentStart = 4 + headerLength;
  if (headerLength === 0 || contentStart > payload.byteLength)
    throw new Error("动态照片数据头无效");
  const header = JSON.parse(
    new TextDecoder().decode(payload.subarray(4, contentStart)),
  ) as LivePhotoBinaryHeader;
  const videoStart = contentStart + header.imageLength;
  if (
    !Number.isSafeInteger(header.imageLength) ||
    header.imageLength <= 0 ||
    videoStart >= payload.byteLength
  ) {
    throw new Error("动态照片媒体长度无效");
  }
  return {
    imageUrl: createDocumentObjectUrl(
      state,
      payload.subarray(contentStart, videoStart),
      header.imageMimeType,
    ),
    imageMimeType: header.imageMimeType,
    videoUrl: createDocumentObjectUrl(state, payload.subarray(videoStart), header.videoMimeType),
    videoMimeType: header.videoMimeType,
    sourceKind: header.sourceKind,
  };
}

export async function loadLocalDocument(
  target: DocumentState,
  path: string,
  options: LoadDocumentOptions = {},
) {
  const generation = beginDocumentLoad(target);
  const state = createDocumentState();
  state.status = "loading";
  try {
    const file =
      options.inspectedFile ?? (await desktopInvoke<OpenedFile>("inspect_file", { path }));
    if (!resolveViewer(file)) throw new Error(`暂不支持 .${file.extension || "未知"} 文件`);
    state.file = file;
    state.viewerKind = file.kind;
    state.sourceModifiedAt = file.modifiedAt;

    if (file.kind === "image") {
      if (["heic", "heif"].includes(file.extension)) {
        const preview = await desktopInvokeBytes("read_heic_preview", { path: file.path });
        state.content = {
          type: "data-url",
          value: createDocumentObjectUrl(state, preview, "image/bmp"),
        };
      } else {
        const image = await desktopInvoke<ImageDocument>("read_image", { path: file.path });
        state.content = { type: "data-url", value: image.dataUrl };
      }
    } else if (file.kind === "live-photo") {
      state.content = { type: "structured", value: await loadLivePhoto(state, file.path) };
    } else if (file.kind === "installer") {
      state.content = {
        type: "structured",
        value: await desktopInvoke<InstallerDocument>("read_installer", { path: file.path }),
      };
    } else if (file.kind === "docx" && file.extension === "doc") {
      const document = await desktopInvoke<HtmlDocument>("read_legacy_doc", { path: file.path });
      state.content = { type: "structured", value: document.html };
    } else if (file.kind === "archive") {
      state.content = {
        type: "structured",
        value: await desktopInvoke<ArchiveDocument>("read_archive", { path: file.path }),
      };
    } else if (file.kind === "ebook" && file.extension === "epub") {
      const document = await desktopInvoke<EpubDocument>("open_epub", { path: file.path });
      registerEbookSession(state, document.sessionId);
      state.content = { type: "structured", value: document };
    } else if (file.kind === "ebook") {
      state.content = {
        type: "bytes",
        value: await desktopInvokeBytes("read_mobi", { path: file.path }),
      };
    } else if (["pdf", "docx", "xlsx", "audio", "video"].includes(file.kind)) {
      state.content = {
        type: "bytes",
        value: await desktopInvokeBytes("read_binary", { path: file.path }),
      };
    } else {
      const result = await desktopInvoke<TextReadResult>("read_text", { path: file.path });
      if (result.status === "large-text") {
        const session = result.session;
        state.encoding = session.encoding;
        state.lineEnding = session.lineEnding;
        state.bom = session.bom;
        state.largeTextSession = session;
        registerLargeTextSession(state, session.sessionId);
        file.encoding = session.encoding;
        file.lineEnding = session.lineEnding;
        file.readOnly = true;
      } else if (result.status !== "success") {
        state.status = "error";
        state.error = {
          code: result.status,
          title: result.status === "binary-content" ? "这是一个二进制文件" : "暂不支持此文本编码",
          message: result.message,
          recoverable: true,
        };
        await commitDocumentLoad(target, state, generation);
        return;
      } else {
        const text = result.document;
        state.content = { type: "text", value: text.content };
        state.encoding = text.encoding;
        state.lineEnding = text.lineEnding;
        state.bom = text.bom;
        state.readOnly = text.readOnly;
        file.encoding = text.encoding;
        file.lineEnding = text.lineEnding;
        file.readOnly = text.readOnly;
      }
    }
    state.readOnly = file.readOnly;
    state.status = "ready";
    if (await commitDocumentLoad(target, state, generation))
      options.onRecent?.(file.path, file.name);
  } catch (error) {
    await disposeDocumentResources(state);
    throw error;
  }
}
