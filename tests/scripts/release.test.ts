import { describe, expect, it } from "vitest";
import { compare, stable } from "../../scripts/release-utils.mjs";

describe("release version comparison and tag validator", () => {
  it("matches valid semver tags", () => {
    expect(stable.test("v1.0.0")).toBe(true);
    expect(stable.test("v1.0.7")).toBe(true);
    expect(stable.test("v10.20.30")).toBe(true);
  });

  it("rejects non-semver or prerelease tags", () => {
    expect(stable.test("1.0.0")).toBe(false);
    expect(stable.test("v1.0.0-beta")).toBe(false);
    expect(stable.test("v01.0.0")).toBe(false);
    expect(stable.test("v1.0")).toBe(false);
    expect(stable.test("")).toBe(false);
  });

  it("correctly compares versions numerically instead of lexicographically", () => {
    expect(compare("v1.0.0", "v1.0.1")).toBe(-1);
    expect(compare("v1.0.1", "v1.0.0")).toBe(1);
    expect(compare("v1.0.0", "v1.0.0")).toBe(0);

    // Multi-digit numbers: 10 > 2
    expect(compare("v1.10.0", "v1.2.0")).toBe(1);
    expect(compare("v1.2.0", "v1.10.0")).toBe(-1);

    // Major and minor differences
    expect(compare("v2.0.0", "v1.99.99")).toBe(1);
    expect(compare("v0.9.9", "v1.0.0")).toBe(-1);
  });
});
