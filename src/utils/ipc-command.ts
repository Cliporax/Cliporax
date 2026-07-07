import { invoke } from "@tauri-apps/api/core";

interface IpcCommandLogger {
  debug: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
}

interface InvokeIpcOptions<T> {
  logger: IpcCommandLogger;
  label: string;
  command: string;
  args?: Record<string, unknown>;
  logArgs?: unknown[];
  onSuccess?: (result: T) => void | Promise<void>;
}

export async function invokeIpc<T>({
  logger,
  label,
  command,
  args,
  logArgs = [],
  onSuccess,
}: InvokeIpcOptions<T>): Promise<T> {
  logger.debug(`${label}() called`, ...logArgs);

  try {
    const result = await invoke<T>(command, args);
    await onSuccess?.(result);
    return result;
  } catch (error) {
    logger.error(`${label}() failed:`, error);
    throw error;
  }
}
