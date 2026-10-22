import type { Component } from "vue";

export type TextEncoding = "utf-8" | "utf-8-bom" | "utf-16-le" | "utf-16-be";
export type LineEnding = "lf" | "crlf" | "cr";

export type ViewerKind =
  | "empty"
  | "image"
  | "live-photo"
  | "text"
  | "markdown"
  | "json"
  | "yaml"
  | "pdf"
  | "docx"
  | "xlsx"
  | "ebook"
  | "archive"
  | "audio"
  | "video"
  | "installer"
  | "binary-structure"
  | "presentation"
  | "hex"
  | "unsupported";

export interface OpenedFile {
  source: "local" | "remote" | "clipboard";
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

export interface RemoteDocument {
  file: OpenedFile;
  content: string;
}

export interface ViewerDefinition {
  kind: ViewerKind;
  componentKey?: string;
  extensions: readonly string[];
  fileNames?: readonly string[];
  readOnly: boolean;
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
  content: DocumentContent;
  dirty: boolean;
  sourceModifiedAt: number | null;
  encoding: TextEncoding | null;
  lineEnding: LineEnding | null;
  bom: boolean;
  readOnly: boolean;
  largeTextSession: LargeTextSessionInfo | null;
  error: DocumentError | null;
}

export type DocumentContent =
  | { type: "empty" }
  | { type: "text"; value: string }
  | { type: "bytes"; value: Uint8Array<ArrayBuffer> }
  | { type: "data-url"; value: string }
  | { type: "structured"; value: unknown };

export interface TextDocument {
  content: string;
  encoding: TextEncoding;
  lineEnding: LineEnding;
  bom: boolean;
  readOnly: boolean;
}

export type TextReadResult =
  | { status: "success"; document: TextDocument }
  | { status: "large-text"; session: LargeTextSessionInfo }
  | { status: "binary-content"; message: string }
  | { status: "unsupported-encoding"; message: string };

export interface ImageDocument {
  dataUrl: string;
  mimeType: string;
}

export interface LivePhotoDocument {
  imageUrl: string;
  imageMimeType: string;
  videoUrl: string;
  videoMimeType: string;
  sourceKind: "livp" | "paired" | "motion-photo";
}

export interface LivePhotoBinaryHeader {
  imageLength: number;
  imageMimeType: string;
  videoMimeType: string;
  sourceKind: LivePhotoDocument["sourceKind"];
}

export interface EpubSpineItem {
  id: string;
  href: string;
  linear: boolean;
}

export interface EpubTocItem {
  label: string;
  href: string;
  chapterId: string | null;
  fragment: string | null;
  children: EpubTocItem[];
}

export interface EpubDocument {
  sessionId: string;
  title: string | null;
  spine: EpubSpineItem[];
  toc: EpubTocItem[];
}

export interface EpubChapter {
  id: string;
  href: string;
  html: string;
}

export interface EpubResource {
  mimeType: string;
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

export interface InstallerDocument {
  format: string;
  name: string | null;
  description: string | null;
  publisher: string | null;
  version: string | null;
  architecture: string | null;
  logoDataUrl: string | null;
}

export interface BinaryChunk {
  offset: number;
  base64: string;
  bytesRead: number;
}

export interface BinaryStructureRow {
  name: string;
  address: string | null;
  offset: string | null;
  size: string | null;
  details: string;
}

export interface BinaryStructureTable {
  items: BinaryStructureRow[];
  truncated: boolean;
}

export interface BinaryStructureDocument {
  format: "PE" | "ELF";
  bitness: number;
  architecture: string;
  byteOrder: string;
  entryPoint: string;
  imageBase: string | null;
  fileType: string;
  sections: BinaryStructureTable;
  imports: BinaryStructureTable;
  exports: BinaryStructureTable;
  symbols: BinaryStructureTable;
  programHeaders: BinaryStructureTable;
  dependencies: BinaryStructureTable;
}

export interface PresentationDocument {
  slides: { index: number; title: string | null; html: string }[];
}

export interface LargeTextLine {
  lineNumber: number;
  content: string;
}

export interface LargeTextLinesResult {
  startLine: number;
  endLine: number;
  lines: LargeTextLine[];
}

export interface LargeTextSessionInfo {
  sessionId: string;
  encoding: TextEncoding;
  lineEnding: LineEnding;
  bom: boolean;
  fileSize: number;
  modifiedAt: number;
  initialLines: LargeTextLine[];
  indexedBytes: number;
  indexedLineCount: number;
}

export interface LargeTextIndexStatus {
  indexedBytes: number;
  fileSize: number;
  indexedLineCount: number;
  complete: boolean;
  totalLines: number | null;
  error: string | null;
}

export interface SaveResult {
  path: string;
  name: string;
  modifiedAt: number;
}
