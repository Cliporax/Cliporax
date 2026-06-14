import { createLogger } from "./logger";

const logger = createLogger("Perf");
const isDev = import.meta.env.DEV;
const lastLogAt = new Map<string, number>();

type PerfFields = Record<string, string | number | boolean | null | undefined>;

interface PerfLogOptions {
  minIntervalMs?: number;
  level?: "info" | "warn";
}

function shouldLog(key: string, minIntervalMs: number): boolean {
  if (!isDev) return false;

  const now = performance.now();
  const last = lastLogAt.get(key) ?? -Infinity;
  if (now - last < minIntervalMs) return false;

  lastLogAt.set(key, now);
  return true;
}

function formatFields(fields?: PerfFields): string {
  if (!fields) return "";

  return Object.entries(fields)
    .filter(([, value]) => value !== undefined)
    .map(([key, value]) => `${key}=${value}`)
    .join(" ");
}

export function perfLog(
  scope: string,
  event: string,
  fields?: PerfFields,
  options: PerfLogOptions = {},
): void {
  const minIntervalMs = options.minIntervalMs ?? 1000;
  const key = `${scope}:${event}`;
  if (!shouldLog(key, minIntervalMs)) return;

  const message = `[${scope}] ${event}${fields ? ` ${formatFields(fields)}` : ""}`;
  if (options.level === "warn") {
    logger.warn(message);
  } else {
    logger.info(message);
  }
}

export function perfMeasure(
  scope: string,
  event: string,
  startTime: number,
  fields?: PerfFields,
  options: PerfLogOptions & { warnAtMs?: number } = {},
): number {
  const durationMs = performance.now() - startTime;
  const roundedDuration = Number(durationMs.toFixed(1));
  const level =
    options.warnAtMs !== undefined && durationMs >= options.warnAtMs
      ? "warn"
      : options.level;

  perfLog(
    scope,
    event,
    { durationMs: roundedDuration, ...fields },
    { ...options, level },
  );

  return durationMs;
}

export function installLongTaskObserver(): () => void {
  if (!isDev || typeof PerformanceObserver === "undefined") {
    return () => {};
  }

  try {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        perfLog(
          "MainThread",
          "long-task",
          {
            durationMs: Number(entry.duration.toFixed(1)),
            startMs: Number(entry.startTime.toFixed(1)),
          },
          { level: "warn", minIntervalMs: 1000 },
        );
      }
    });

    observer.observe({ entryTypes: ["longtask"] });
    return () => observer.disconnect();
  } catch {
    return () => {};
  }
}
