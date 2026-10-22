import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  DesktopCommandError,
  desktopInvoke,
  desktopInvokeBytes,
  normalizeCommandError,
} from "../../src/services/desktop-api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("desktop-api IPC invocation and error handling", () => {
  it("creates DesktopCommandError with correct properties", () => {
    const error = new DesktopCommandError({
      code: "file-not-found",
      message: "The requested file does not exist",
      recoverable: true,
    });

    expect(error).toBeInstanceOf(Error);
    expect(error).toBeInstanceOf(DesktopCommandError);
    expect(error.name).toBe("DesktopCommandError");
    expect(error.code).toBe("file-not-found");
    expect(error.message).toBe("The requested file does not exist");
    expect(error.recoverable).toBe(true);
  });

  it("normalizes structured CommandError from Rust backend", () => {
    const backendPayload = {
      code: "permission-denied",
      message: "Access to path is denied",
      recoverable: false,
    };
    const normalized = normalizeCommandError(backendPayload);

    expect(normalized).toEqual({
      code: "permission-denied",
      message: "Access to path is denied",
      recoverable: false,
    });
  });

  it("falls back to desktop-command-failed for generic Error objects and unknown values", () => {
    const genericError = new Error("Network timeout");
    expect(normalizeCommandError(genericError)).toEqual({
      code: "desktop-command-failed",
      message: "Network timeout",
      recoverable: true,
    });

    expect(normalizeCommandError("raw failure string")).toEqual({
      code: "desktop-command-failed",
      message: "raw failure string",
      recoverable: true,
    });

    expect(normalizeCommandError(null)).toEqual({
      code: "desktop-command-failed",
      message: "null",
      recoverable: true,
    });
  });

  it("returns parsed result when desktopInvoke succeeds", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ status: "ok", data: 42 });

    const result = await desktopInvoke<{ status: string; data: number }>("test_command", { key: "value" });

    expect(invoke).toHaveBeenCalledWith("test_command", { key: "value" });
    expect(result).toEqual({ status: "ok", data: 42 });
  });

  it("converts backend structured error and throws DesktopCommandError on desktopInvoke failure", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "invalid-argument",
      message: "Session ID is invalid",
      recoverable: false,
    });

    const result = desktopInvoke("close_session", { sessionId: "bad-id" });
    await expect(result).rejects.toBeInstanceOf(DesktopCommandError);
    await expect(result).rejects.toMatchObject({
      code: "invalid-argument",
      message: "Session ID is invalid",
      recoverable: false,
    });
  });

  it("converts generic rejection into desktop-command-failed and rethrows", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("IPC connection lost"));

    try {
      await desktopInvoke("read_file");
      expect.unreachable("desktopInvoke should have thrown");
    } catch (error) {
      expect(error).toBeInstanceOf(DesktopCommandError);
      const cmdErr = error as DesktopCommandError;
      expect(cmdErr.code).toBe("desktop-command-failed");
      expect(cmdErr.message).toBe("IPC connection lost");
      expect(cmdErr.recoverable).toBe(true);
    }
  });

  it("converts ArrayBuffer to Uint8Array when desktopInvokeBytes succeeds", async () => {
    const buffer = new Uint8Array([1, 2, 3, 4]).buffer;
    vi.mocked(invoke).mockResolvedValueOnce(buffer);

    const bytes = await desktopInvokeBytes("read_binary", { path: "sample.bin" });

    expect(bytes).toBeInstanceOf(Uint8Array);
    expect(Array.from(bytes)).toEqual([1, 2, 3, 4]);
  });
});
