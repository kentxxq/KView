import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DocumentState } from "../../src/types/document";
import { desktopInvoke } from "../../src/services/desktop-api";
import {
  beginDocumentLoad,
  commitDocumentLoad,
  createDocumentObjectUrl,
  disposeDocumentResources,
  isCurrentDocumentLoad,
  registerEbookSession,
  registerLargeTextSession,
} from "../../src/services/document-resources";

vi.mock("../../src/services/desktop-api", () => ({
  desktopInvoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../src/services/diagnostics", () => ({
  recordDiagnostic: vi.fn(),
}));

function createMockDocumentState(overrides: Partial<DocumentState> = {}): DocumentState {
  return {
    file: {
      source: "local",
      path: "D:\\test\\sample.txt",
      name: "sample.txt",
      extension: "txt",
      size: 1024,
      modifiedAt: 1700000000000,
      kind: "text",
      encoding: "utf-8",
      lineEnding: "lf",
      readOnly: false,
    },
    status: "ready",
    viewerKind: "text",
    content: { type: "text", value: "initial content" },
    dirty: false,
    sourceModifiedAt: 1700000000000,
    encoding: "utf-8",
    lineEnding: "lf",
    bom: false,
    readOnly: false,
    largeTextSession: null,
    error: null,
    ...overrides,
  };
}

describe("document-resources lifetime, handle tracking and generation guards", () => {
  let createdUrls: string[] = [];
  let revokedUrls: string[] = [];

  beforeEach(() => {
    vi.clearAllMocks();
    createdUrls = [];
    revokedUrls = [];

    let counter = 0;
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => {
        const url = `blob:http://localhost/mock-blob-${++counter}`;
        createdUrls.push(url);
        return url;
      }),
      revokeObjectURL: vi.fn((url: string) => {
        revokedUrls.push(url);
      }),
    });
  });

  it("increments generation numbers monotonically on each beginDocumentLoad", () => {
    const doc = createMockDocumentState();

    const gen1 = beginDocumentLoad(doc);
    expect(gen1).toBe(1);
    expect(isCurrentDocumentLoad(doc, 1)).toBe(true);
    expect(isCurrentDocumentLoad(doc, 0)).toBe(false);

    const gen2 = beginDocumentLoad(doc);
    expect(gen2).toBe(2);
    expect(isCurrentDocumentLoad(doc, 2)).toBe(true);
    // Previous generation must be immediately invalidated
    expect(isCurrentDocumentLoad(doc, 1)).toBe(false);
  });

  it("isolates generation tracking across different document states", () => {
    const docA = createMockDocumentState();
    const docB = createMockDocumentState();

    const genA1 = beginDocumentLoad(docA);
    const genB1 = beginDocumentLoad(docB);
    const genA2 = beginDocumentLoad(docA);

    expect(genA1).toBe(1);
    expect(genB1).toBe(1);
    expect(genA2).toBe(2);

    expect(isCurrentDocumentLoad(docA, 2)).toBe(true);
    expect(isCurrentDocumentLoad(docA, 1)).toBe(false);
    expect(isCurrentDocumentLoad(docB, 1)).toBe(true);
  });

  it("registers and revokes object URLs on disposeDocumentResources", async () => {
    const doc = createMockDocumentState();
    const bytes = new Uint8Array([1, 2, 3]);

    const url1 = createDocumentObjectUrl(doc, bytes, "image/png");
    const url2 = createDocumentObjectUrl(doc, bytes, "image/png");

    expect(createdUrls).toContain(url1);
    expect(createdUrls).toContain(url2);

    await disposeDocumentResources(doc);

    expect(revokedUrls).toContain(url1);
    expect(revokedUrls).toContain(url2);
  });

  it("closes backend sessions and resets largeTextSession on disposeDocumentResources", async () => {
    const doc = createMockDocumentState({
      largeTextSession: {
        sessionId: "large-text-101",
        encoding: "utf-8",
        lineEnding: "lf",
        bom: false,
        fileSize: 20000000,
        modifiedAt: 1700000000000,
        initialLines: [],
        indexedBytes: 20000000,
        indexedLineCount: 50000,
      },
    });

    registerLargeTextSession(doc, "large-text-101");
    registerEbookSession(doc, "ebook-202");

    await disposeDocumentResources(doc);

    expect(doc.largeTextSession).toBeNull();
    expect(desktopInvoke).toHaveBeenCalledWith("close_large_text_session", { sessionId: "large-text-101" });
    expect(desktopInvoke).toHaveBeenCalledWith("close_epub", { sessionId: "ebook-202" });
  });

  it("rejects commitDocumentLoad when initial generation is outdated", async () => {
    const target = createMockDocumentState({ content: { type: "text", value: "target initial" } });
    const loaded = createMockDocumentState({ content: { type: "text", value: "loaded content" } });

    const gen1 = beginDocumentLoad(target);
    const gen2 = beginDocumentLoad(target); // user switched away
    expect(gen2).toBe(2);

    const committed = await commitDocumentLoad(target, loaded, gen1);
    expect(committed).toBe(false);
    expect(target.content).toEqual({ type: "text", value: "target initial" });
  });

  it("transfers resources and updates target on successful commitDocumentLoad", async () => {
    const target = createMockDocumentState({ content: { type: "text", value: "target old" } });
    const loaded = createMockDocumentState({ content: { type: "text", value: "loaded fresh" } });

    // target had an old URL
    const targetOldUrl = createDocumentObjectUrl(target, new Uint8Array([0]), "text/plain");
    // loaded has a fresh URL
    const loadedFreshUrl = createDocumentObjectUrl(loaded, new Uint8Array([1]), "text/plain");

    const gen = beginDocumentLoad(target);
    const success = await commitDocumentLoad(target, loaded, gen);

    expect(success).toBe(true);
    expect(target.content).toEqual({ type: "text", value: "loaded fresh" });

    // target's old URL must have been revoked during commit
    expect(revokedUrls).toContain(targetOldUrl);
    expect(revokedUrls).not.toContain(loadedFreshUrl);

    // Now disposing target should revoke the transferred fresh URL
    await disposeDocumentResources(target);
    expect(revokedUrls).toContain(loadedFreshUrl);
  });

  it("handles race condition when new load begins during async target release", async () => {
    const target = createMockDocumentState();
    const loaded = createMockDocumentState({ content: { type: "text", value: "loaded" } });

    const loadedUrl = createDocumentObjectUrl(loaded, new Uint8Array([1]), "text/plain");

    // Register a session on target that causes an asynchronous desktopInvoke call during release
    registerLargeTextSession(target, "session-race");

    let resolveSessionClose: () => void;
    const sessionClosePromise = new Promise<void>((resolve) => {
      resolveSessionClose = resolve;
    });

    vi.mocked(desktopInvoke).mockImplementation(async (command) => {
      if (command === "close_large_text_session") {
        await sessionClosePromise;
      }
      return undefined as never;
    });

    const gen1 = beginDocumentLoad(target);

    // Start commitDocumentLoad with gen1
    const commitPromise = commitDocumentLoad(target, loaded, gen1);

    // While releaseHandles(target) is awaiting close_large_text_session, a new load starts on target!
    const gen2 = beginDocumentLoad(target);
    expect(gen2).toBe(2);

    // Now let target release finish
    resolveSessionClose!();

    const commitResult = await commitPromise;

    // Second guard check must detect generation mismatch and reject commit
    expect(commitResult).toBe(false);

    // Loaded resources must be cleaned up because commit was aborted
    expect(revokedUrls).toContain(loadedUrl);
  });
});
