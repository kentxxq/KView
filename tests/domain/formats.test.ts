import { describe, expect, it } from "vitest";
import {
  associationGroupsForPlatform,
  capabilitiesForPlatform,
  formatCapabilities,
} from "../../src/domain/formats";

describe("formats and platform capabilities", () => {
  it("loads non-empty catalog of format capabilities", () => {
    expect(formatCapabilities.length).toBeGreaterThan(0);
    for (const item of formatCapabilities) {
      expect(item.id).toBeTruthy();
      expect(item.label).toBeTruthy();
      expect(item.viewerKind).toBeTruthy();
      expect(Array.isArray(item.platforms)).toBe(true);
      expect(item.platforms.length).toBeGreaterThan(0);
    }
  });

  it("filters capabilities correctly for windows and macos platforms", () => {
    const windowsCaps = capabilitiesForPlatform("windows");
    const macosCaps = capabilitiesForPlatform("macos");

    expect(windowsCaps.length).toBeGreaterThan(0);
    expect(macosCaps.length).toBeGreaterThan(0);

    for (const cap of windowsCaps) {
      expect(cap.platforms).toContain("windows");
    }
    for (const cap of macosCaps) {
      expect(cap.platforms).toContain("macos");
    }
  });

  it("deduplicates extensions in association groups", () => {
    const winGroups = associationGroupsForPlatform("windows");
    expect(winGroups.length).toBeGreaterThan(0);

    for (const group of winGroups) {
      expect(group.label).toBeTruthy();
      expect(group.extensions.length).toBeGreaterThan(0);

      // Verify all extensions in group are unique
      const uniqueExtensions = new Set(group.extensions);
      expect(uniqueExtensions.size).toBe(group.extensions.length);
    }
  });

  it("excludes non-associable capabilities from association groups", () => {
    const winGroups = associationGroupsForPlatform("windows");
    const nonAssociableLabels = new Set(
      formatCapabilities.filter((c) => !c.associable).map((c) => c.label),
    );

    for (const group of winGroups) {
      expect(nonAssociableLabels.has(group.label)).toBe(false);
    }
  });
});
