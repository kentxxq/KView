import type { Component } from "vue";

export type TextEncoding = "utf-8" | "utf-8-bom" | "utf-16-le" | "utf-16-be";
export type LineEnding = "lf" | "crlf" | "cr";

export type ViewerKind =
  | "empty"
  | "image"
  | "text"
  | "markdown"
  | "json"
  | "yaml"
  | "pdf"
  | "docx"
  | "xlsx"
  | "archive"
  | "video"
  | "hex"
  | "unsupported";

export interface OpenedFile {
  path: string;
  name: string;
  extension: string;
  size: number;
  modifiedAt: number;
  kind: ViewerKind;
  encoding: TextEncoding | null;
  lineEnding: LineEnding | null;
  readOnly: boolean;
}

export interface ViewerDefinition {
  kind: ViewerKind;
  extensions: readonly string[];
  fileNames?: readonly string[];
  readOnly: boolean;
  maxEditableBytes: number | null;
  load: () => Promise<{ default: Component }>;
}

export interface DocumentError {
  code: string;
  title: string;
  message: string;
  recoverable: boolean;
}

export interface DocumentState {
  file: OpenedFile | null;
  status: "empty" | "loading" | "ready" | "error";
  viewerKind: ViewerKind;
  buffer: string | null;
  dirty: boolean;
  sourceModifiedAt: number | null;
  encoding: TextEncoding | null;
  lineEnding: LineEnding | null;
  bom: boolean;
  readOnly: boolean;
  truncated: boolean;
  error: DocumentError | null;
}

export interface TextDocument {
  content: string;
  encoding: TextEncoding;
  lineEnding: LineEnding;
  bom: boolean;
  readOnly: boolean;
  truncated: boolean;
}

export type TextReadResult =
  | { status: "success"; document: TextDocument }
  | { status: "binary-content"; message: string }
  | { status: "unsupported-encoding"; message: string };

export interface ImageDocument {
  dataUrl: string;
  mimeType: string;
}

export interface BinaryDocument {
  base64: string;
}

export interface HtmlDocument {
  html: string;
}

export interface ArchiveEntry {
  path: string;
  size: number;
  compressedSize: number | null;
  isDirectory: boolean;
}

export interface ArchiveDocument {
  format: string;
  entries: ArchiveEntry[];
  truncated: boolean;
}

export interface BinaryChunk {
  offset: number;
  base64: string;
  bytesRead: number;
}

export interface SaveResult {
  path: string;
  name: string;
  modifiedAt: number;
}
