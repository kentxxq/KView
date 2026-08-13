import catalog from "../../shared/formats.json";
import type { ViewerKind } from "../types/document";

export type DesktopPlatform = "windows" | "macos";

export interface FormatCapability {
  id: string;
  label: string;
  viewerKind: ViewerKind;
  extensions: string[];
  fileNames: string[];
  fileSuffixes?: string[];
  associationExtensions?: string[];
  platforms: DesktopPlatform[];
  editable: boolean;
  associable: boolean;
  hexFallback: boolean;
}

export const formatCapabilities = catalog as FormatCapability[];

export function currentDesktopPlatform(): DesktopPlatform | null {
  const identity = `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
  if (identity.includes("win")) return "windows";
  if (identity.includes("mac")) return "macos";
  return null;
}

export function capabilitiesForPlatform(platform: DesktopPlatform) {
  return formatCapabilities.filter((capability) => capability.platforms.includes(platform));
}

export function associationGroupsForPlatform(platform: DesktopPlatform) {
  const groups = new Map<string, string[]>();
  for (const capability of capabilitiesForPlatform(platform)) {
    if (!capability.associable) continue;
    const extensions = groups.get(capability.label) ?? [];
    extensions.push(...capability.extensions, ...(capability.associationExtensions ?? []));
    groups.set(capability.label, extensions);
  }
  return [...groups].map(([label, extensions]) => ({
    label,
    extensions: [...new Set(extensions)],
  }));
}
