// Centralized logging utility with environment-aware output and trace support
import { invoke } from "@tauri-apps/api/core";
import { TraceContext } from "./trace-context";

const isDev = import.meta.env.DEV;

export enum LogLevel {
  DEBUG = 0,
  INFO = 1,
  WARN = 2,
  ERROR = 3,
}

// In production, only show WARN and ERROR
const currentLogLevel = isDev ? LogLevel.DEBUG : LogLevel.WARN;

/**
 * Log entry structure for backend transmission
 */
interface LogEntry {
  level: string;
  component: string;
  message: string;
  trace_id?: string | null;
  span_id?: string | null;
  timestamp?: string;
}

class Logger {
  private component: string;

  constructor(component: string) {
    this.component = component;
  }

  private shouldLog(level: LogLevel): boolean {
    return level >= currentLogLevel;
  }

  /**
   * Format log message with trace context (human-readable)
   */
  private formatMessage(level: LogLevel, args: any[]): string {
    const message = args
      .map((a) => (typeof a === "object" ? JSON.stringify(a) : String(a)))
      .join(" ");

    if (!isDev) {
      return message;
    }

    // In dev mode, add trace information
    const traceCtx = TraceContext.getInstance();
    const traceId = traceCtx.getTraceId();
    const spanId = traceCtx.getSpanId();

    if (traceId) {
      return `[TRACE:${traceId}] [SPAN:${spanId}] ${message}`;
    }

    return message;
  }

  /**
   * Send log to backend file (dev mode only)
   */
  private writeToBackend(level: LogLevel, args: any[]) {
    if (!isDev) return;

    const message = args
      .map((a) => (typeof a === "object" ? JSON.stringify(a) : String(a)))
      .join(" ");

    const traceCtx = TraceContext.getInstance();
    const context = traceCtx.getContext();

    const entry: LogEntry = {
      level: LogLevel[level],
      component: this.component,
      message,
      trace_id: context.trace_id,
      span_id: context.span_id,
      timestamp: new Date().toISOString(),
    };

    // Fire-and-forget, don't wait for response
    Promise.resolve(invoke("dev_log_write", {
      entry,
      _trace_id: context.trace_id,
      _span_id: context.span_id,
    })).catch((err) => {
      // Log error to console for debugging
      console.error("[Logger] Failed to write to backend:", err);
    });
  }

  debug(...args: any[]) {
    if (this.shouldLog(LogLevel.DEBUG)) {
      const formatted = this.formatMessage(LogLevel.DEBUG, args);
      console.log(`[${this.component}] DEBUG:`, formatted);
      this.writeToBackend(LogLevel.DEBUG, args);
    }
  }

  info(...args: any[]) {
    if (this.shouldLog(LogLevel.INFO)) {
      const formatted = this.formatMessage(LogLevel.INFO, args);
      console.log(`[${this.component}] INFO:`, formatted);
      this.writeToBackend(LogLevel.INFO, args);
    }
  }

  warn(...args: any[]) {
    if (this.shouldLog(LogLevel.WARN)) {
      const formatted = this.formatMessage(LogLevel.WARN, args);
      console.warn(`[${this.component}] WARN:`, formatted);
      this.writeToBackend(LogLevel.WARN, args);
    }
  }

  error(...args: any[]) {
    if (this.shouldLog(LogLevel.ERROR)) {
      const formatted = this.formatMessage(LogLevel.ERROR, args);
      console.error(`[${this.component}] ERROR:`, formatted);
      this.writeToBackend(LogLevel.ERROR, args);
    }
  }

  /**
   * Create a traced logger with explicit trace context
   * For backward compatibility and manual trace control
   */
  withTrace(traceId: string, spanId: string): TracedLogger {
    return new TracedLogger(this.component, traceId, spanId);
  }
}

/**
 * Traced Logger - logs with explicit trace context
 * Used for backward compatibility and special cases
 */
class TracedLogger {
  private component: string;
  private traceId: string;
  private spanId: string;

  constructor(component: string, traceId: string, spanId: string) {
    this.component = component;
    this.traceId = traceId;
    this.spanId = spanId;
  }

  private formatMessage(level: string, args: any[]): string {
    const message = args
      .map((a) => (typeof a === "object" ? JSON.stringify(a) : String(a)))
      .join(" ");

    return `[TRACE:${this.traceId}] [SPAN:${this.spanId}] ${message}`;
  }

  debug(...args: any[]) {
    if (!isDev) return;
    const formatted = this.formatMessage("DEBUG", args);
    console.log(`[${this.component}] DEBUG:`, formatted);
  }

  info(...args: any[]) {
    if (!isDev) return;
    const formatted = this.formatMessage("INFO", args);
    console.log(`[${this.component}] INFO:`, formatted);
  }

  warn(...args: any[]) {
    if (!isDev) return;
    const formatted = this.formatMessage("WARN", args);
    console.warn(`[${this.component}] WARN:`, formatted);
  }

  error(...args: any[]) {
    if (!isDev) return;
    const formatted = this.formatMessage("ERROR", args);
    console.error(`[${this.component}] ERROR:`, formatted);
  }
}

export const createLogger = (component: string) => new Logger(component);
