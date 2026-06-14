/**
 * Trace Context Manager
 *
 * Manages distributed tracing context for cross-frontend/backend event correlation.
 * Implements OpenTelemetry-style trace_id + span_id model.
 *
 * Usage:
 * ```typescript
 * // Start a new trace
 * const traceId = TraceContext.getInstance().startTrace('Settings', 'card_size_change');
 *
 * // Create child span for IPC call
 * const childSpan = TraceContext.getInstance().createChildSpan();
 *
 * // Get current context (auto-includes in IPC calls)
 * const ctx = TraceContext.getInstance().getContext();
 *
 * // Clear trace when operation completes
 * TraceContext.getInstance().clearTrace();
 * ```
 */

interface TraceContextData {
  trace_id: string | null;
  span_id: string | null;
  parent_span_id: string | null;
  sequence: number;
  component: string | null;
  action: string | null;
  startTime: number | null;
}

export class TraceContext {
  private static instance: TraceContext | null = null;
  private context: TraceContextData;

  private constructor() {
    this.context = {
      trace_id: null,
      span_id: null,
      parent_span_id: null,
      sequence: 0,
      component: null,
      action: null,
      startTime: null,
    };
  }

  /**
   * Get singleton instance
   */
  static getInstance(): TraceContext {
    if (!TraceContext.instance) {
      TraceContext.instance = new TraceContext();
    }
    return TraceContext.instance;
  }

  /**
   * Start a new trace
   * @param component Component/module name
   * @param action Action description
   * @returns trace_id
   */
  startTrace(component: string, action: string): string {
    // Generate 8-char UUID for trace_id
    this.context.trace_id = this.generateShortUUID();
    this.context.span_id = "s1";
    this.context.parent_span_id = null;
    this.context.sequence = 1;
    this.context.component = component;
    this.context.action = action;
    this.context.startTime = Date.now();

    return this.context.trace_id;
  }

  /**
   * Create a child span for sub-operations
   * @returns child span_id
   */
  createChildSpan(): string {
    if (!this.context.trace_id) {
      // Auto-start trace if not exists
      this.startTrace("Auto", "auto_trace");
    }

    const parentSpan = this.context.span_id || "s1";
    this.context.sequence++;
    const childSpan = `${parentSpan}.${this.context.sequence}`;
    this.context.parent_span_id = this.context.span_id;
    this.context.span_id = childSpan;

    return childSpan;
  }

  /**
   * Get current trace context
   */
  getContext(): {
    trace_id: string | null;
    span_id: string | null;
    parent_span_id: string | null;
    sequence: number;
  } {
    return {
      trace_id: this.context.trace_id,
      span_id: this.context.span_id,
      parent_span_id: this.context.parent_span_id,
      sequence: this.context.sequence,
    };
  }

  /**
   * Get current trace_id
   */
  getTraceId(): string | null {
    return this.context.trace_id;
  }

  /**
   * Get current span_id
   */
  getSpanId(): string | null {
    return this.context.span_id;
  }

  /**
   * Check if currently in a trace
   */
  hasActiveTrace(): boolean {
    return this.context.trace_id !== null;
  }

  /**
   * Get trace metadata
   */
  getMetadata(): {
    component: string | null;
    action: string | null;
    duration_ms: number | null;
  } {
    const duration_ms = this.context.startTime
      ? Date.now() - this.context.startTime
      : null;

    return {
      component: this.context.component,
      action: this.context.action,
      duration_ms,
    };
  }

  /**
   * Clear current trace (call when operation completes)
   */
  clearTrace(): void {
    this.context = {
      trace_id: null,
      span_id: null,
      parent_span_id: null,
      sequence: 0,
      component: null,
      action: null,
      startTime: null,
    };
  }

  /**
   * Reset instance (for testing)
   */
  static resetInstance(): void {
    TraceContext.instance = null;
  }

  /**
   * Generate 8-character short UUID
   */
  private generateShortUUID(): string {
    if (typeof crypto !== "undefined" && crypto.randomUUID) {
      return crypto.randomUUID().replace(/-/g, "").slice(0, 8);
    }
    // Fallback for older environments
    return Math.random().toString(36).substring(2, 10);
  }
}
