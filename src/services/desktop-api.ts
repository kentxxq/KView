import { invoke } from "@tauri-apps/api/core";

export interface CommandErrorShape {
  code: string;
  message: string;
  recoverable: boolean;
}

export class DesktopCommandError extends Error implements CommandErrorShape {
  code: string;
  recoverable: boolean;

  constructor(error: CommandErrorShape) {
    super(error.message);
    this.name = "DesktopCommandError";
    this.code = error.code;
    this.recoverable = error.recoverable;
  }
}

function normalizeCommandError(cause: unknown): CommandErrorShape {
  if (cause && typeof cause === "object") {
    const value = cause as Partial<CommandErrorShape>;
    if (typeof value.code === "string" && typeof value.message === "string") {
      return { code: value.code, message: value.message, recoverable: value.recoverable !== false };
    }
  }
  const message = cause instanceof Error ? cause.message : String(cause);
  return { code: "desktop-command-failed", message, recoverable: true };
}

export async function desktopInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (cause) {
    throw new DesktopCommandError(normalizeCommandError(cause));
  }
}

export async function desktopInvokeBytes(command: string, args: Record<string, unknown>) {
  const response = await desktopInvoke<ArrayBuffer>(command, args);
  return new Uint8Array<ArrayBuffer>(response);
}
