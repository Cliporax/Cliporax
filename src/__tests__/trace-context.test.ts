/**
 * TraceContext Unit Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { TraceContext } from "../utils/trace-context";

describe("TraceContext", () => {
  beforeEach(() => {
    TraceContext.resetInstance();
  });

  it("should create singleton instance", () => {
    const instance1 = TraceContext.getInstance();
    const instance2 = TraceContext.getInstance();
    expect(instance1).toBe(instance2);
  });

  it("should generate trace_id on startTrace", () => {
    const ctx = TraceContext.getInstance();
    const traceId = ctx.startTrace("Test", "test_action");

    expect(traceId).toBeTruthy();
    expect(traceId.length).toBe(8);
    expect(ctx.getTraceId()).toBe(traceId);
  });

  it("should initialize span_id to s1", () => {
    const ctx = TraceContext.getInstance();
    ctx.startTrace("Test", "test_action");

    expect(ctx.getSpanId()).toBe("s1");
  });

  it("should create child spans", () => {
    const ctx = TraceContext.getInstance();
    ctx.startTrace("Test", "test_action");

    const childSpan = ctx.createChildSpan();
    expect(childSpan).toBe("s1.2");
    expect(ctx.getSpanId()).toBe("s1.2");
    expect(ctx.getContext().parent_span_id).toBe("s1");
  });

  it("should increment sequence number", () => {
    const ctx = TraceContext.getInstance();
    ctx.startTrace("Test", "test_action");

    expect(ctx.getContext().sequence).toBe(1);

    ctx.createChildSpan();
    expect(ctx.getContext().sequence).toBe(2);

    ctx.createChildSpan();
    expect(ctx.getContext().sequence).toBe(3);
  });

  it("should return full context", () => {
    const ctx = TraceContext.getInstance();
    ctx.startTrace("Test", "test_action");
    ctx.createChildSpan();

    const context = ctx.getContext();
    expect(context.trace_id).toBeTruthy();
    expect(context.span_id).toBe("s1.2");
    expect(context.parent_span_id).toBe("s1");
    expect(context.sequence).toBe(2);
  });

  it("should track trace metadata", () => {
    const ctx = TraceContext.getInstance();
    ctx.startTrace("Settings", "update_card_size");

    const metadata = ctx.getMetadata();
    expect(metadata.component).toBe("Settings");
    expect(metadata.action).toBe("update_card_size");
    expect(metadata.duration_ms).toBeGreaterThanOrEqual(0);
  });

  it("should clear trace", () => {
    const ctx = TraceContext.getInstance();
    ctx.startTrace("Test", "test_action");

    expect(ctx.hasActiveTrace()).toBe(true);

    ctx.clearTrace();

    expect(ctx.hasActiveTrace()).toBe(false);
    expect(ctx.getTraceId()).toBeNull();
    expect(ctx.getSpanId()).toBeNull();
  });

  it("should auto-start trace on createChildSpan if not exists", () => {
    const ctx = TraceContext.getInstance();

    // Don't call startTrace, directly create child span
    const childSpan = ctx.createChildSpan();

    expect(ctx.hasActiveTrace()).toBe(true);
    expect(childSpan).toBeTruthy();
  });

  it("should generate unique trace_ids", () => {
    const ctx = TraceContext.getInstance();

    const traceId1 = ctx.startTrace("Test", "action1");
    ctx.clearTrace();

    const traceId2 = ctx.startTrace("Test", "action2");

    expect(traceId1).not.toBe(traceId2);
  });
});
