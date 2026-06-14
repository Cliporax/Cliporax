/**
 * Traced IPC Invoke Wrapper
 *
 * Automatically injects trace context into all IPC calls and logs
 * the complete lifecycle (CALL → RETURN/ERROR) with duration tracking.
 *
 * This enables cross-frontend/backend event correlation via trace_id.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { TraceContext } from "./trace-context";
import { createLogger } from "./logger";

const ipcLogger = createLogger("IPC");

/**
 * Enhanced invoke with automatic trace context injection
 *
 * @param command Tauri command name
 * @param args Command arguments (trace context will be auto-injected)
 * @returns Command result
 *
 * @example
 * ```typescript
 * // Start a trace for user action
 * TraceContext.getInstance().startTrace('Settings', 'update_card_size')
 *
 * // IPC call automatically includes trace context
 * await tracedInvoke('settings_update', { newSettings })
 *
 * // Clear trace when done
 * TraceContext.getInstance().clearTrace()
 * ```
 */
export async function tracedInvoke<T>(
  command: string,
  args: Record<string, any> = {},
): Promise<T> {
  const traceCtx = TraceContext.getInstance();
  const context = traceCtx.getContext();

  // Auto-inject trace context into arguments
  const tracedArgs = {
    ...args,
    _trace_id: context.trace_id,
    _span_id: context.span_id,
    _parent_span_id: context.parent_span_id,
    _sequence: context.sequence,
  };

  // Log IPC call start
  ipcLogger.debug(`CALL ${command}`, {
    trace_id: context.trace_id,
    span_id: context.span_id,
  });

  const startTime = Date.now();
  try {
    const result = await tauriInvoke<T>(command, tracedArgs);
    const duration = Date.now() - startTime;

    // Log IPC call success
    ipcLogger.debug(`RETURN ${command} (${duration}ms)`, {
      trace_id: context.trace_id,
      span_id: context.span_id,
      duration_ms: duration,
    });

    return result;
  } catch (error) {
    const duration = Date.now() - startTime;

    // Log IPC call error
    ipcLogger.error(`ERROR ${command} (${duration}ms)`, {
      trace_id: context.trace_id,
      span_id: context.span_id,
      duration_ms: duration,
      error: error instanceof Error ? error.message : String(error),
    });

    throw error;
  }
}

/**
 * Create a traced IPC call with explicit trace context
 * Used when you need manual control over trace_id/span_id
 *
 * @param command Tauri command name
 * @param args Command arguments
 * @param traceId Explicit trace ID
 * @param spanId Explicit span ID
 */
export async function tracedInvokeWith<T>(
  command: string,
  args: Record<string, any> = {},
  traceId: string,
  spanId: string,
): Promise<T> {
  const tracedArgs = {
    ...args,
    _trace_id: traceId,
    _span_id: spanId,
  };

  ipcLogger.debug(`CALL ${command} [TRACE:${traceId}] [SPAN:${spanId}]`);

  const startTime = Date.now();
  try {
    const result = await tauriInvoke<T>(command, tracedArgs);
    const duration = Date.now() - startTime;

    ipcLogger.debug(`RETURN ${command} (${duration}ms) [TRACE:${traceId}]`);

    return result;
  } catch (error) {
    const duration = Date.now() - startTime;

    ipcLogger.error(`ERROR ${command} (${duration}ms) [TRACE:${traceId}]`, {
      error: error instanceof Error ? error.message : String(error),
    });

    throw error;
  }
}

/**
 * Helper: Execute a function within a trace context
 * Automatically manages trace lifecycle (start → execute → clear)
 *
 * @param component Component name
 * @param action Action description
 * @param fn Async function to execute
 * @returns Function result
 *
 * @example
 * ```typescript
 * const result = await withTrace('Settings', 'update', async () => {
 *   return await tracedInvoke('settings_update', { newSettings })
 * })
 * ```
 */
export async function withTrace<T>(
  component: string,
  action: string,
  fn: () => Promise<T>,
): Promise<T> {
  const traceCtx = TraceContext.getInstance();

  try {
    traceCtx.startTrace(component, action);
    const result = await fn();
    return result;
  } finally {
    traceCtx.clearTrace();
  }
}
