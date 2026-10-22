import type { BinaryStructureDocument, OpenedFile } from "../types/document";
import { desktopInvoke } from "./desktop-api";

export function readBinaryStructure(file: OpenedFile) {
  return desktopInvoke<BinaryStructureDocument>("read_binary_structure", {
    path: file.path,
    expectedModifiedAt: file.modifiedAt,
  });
}
