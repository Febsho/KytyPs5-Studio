import { useEffect } from "react";
import type { ProcessMetric } from "../types/studio";
import type { StudioCommand } from "./useStudioData";

const PROCESS_METRICS_REFRESH_EVENT = "studio:refresh-process-metrics";

export function processMetricsPollDelay(active: boolean, visible: boolean) {
  // Process telemetry is fed into the app shell, so each poll can repaint the
  // whole window. Keep live-session stats useful without forcing a 1 Hz render.
  if (active) return visible ? 5_000 : 15_000;
  return visible ? 15_000 : 60_000;
}

export function refreshProcessMetricsNow() {
  window.dispatchEvent(new Event(PROCESS_METRICS_REFRESH_EVENT));
}

export function useProcessMetrics(
  call: StudioCommand,
  onMetrics: (metrics: ProcessMetric[]) => void,
  onRecovery: () => void,
  onError: (error: unknown) => void,
) {
  useEffect(() => {
    let disposed = false;
    let inFlight = false;
    let refreshRequested = false;
    let active = false;
    let timer: number | undefined;

    const schedule = () => {
      if (disposed) return;
      if (timer !== undefined) window.clearTimeout(timer);
      timer = window.setTimeout(
        () => void poll(),
        processMetricsPollDelay(
          active,
          document.visibilityState !== "hidden",
        ),
      );
    };

    const poll = async () => {
      if (disposed) return;
      if (inFlight) {
        refreshRequested = true;
        return;
      }
      inFlight = true;
      try {
        const metrics = await call<ProcessMetric[]>("process_metrics");
        if (disposed) return;
        onMetrics(metrics);
        const ended = metrics.some((metric) =>
          /(ended|interrupted)/i.test(metric.status),
        );
        active = metrics.some(
          (metric) => !/(ended|interrupted)/i.test(metric.status),
        );
        if (ended) onRecovery();
      } catch (error) {
        if (!disposed) onError(error);
      } finally {
        inFlight = false;
        if (disposed) return;
        if (refreshRequested) {
          refreshRequested = false;
          void poll();
        } else {
          schedule();
        }
      }
    };

    const refreshNow = () => {
      if (timer !== undefined) window.clearTimeout(timer);
      void poll();
    };
    const visibilityChanged = () => {
      if (document.visibilityState === "visible") refreshNow();
      else schedule();
    };

    window.addEventListener(PROCESS_METRICS_REFRESH_EVENT, refreshNow);
    document.addEventListener("visibilitychange", visibilityChanged);
    void poll();
    return () => {
      disposed = true;
      if (timer !== undefined) window.clearTimeout(timer);
      window.removeEventListener(PROCESS_METRICS_REFRESH_EVENT, refreshNow);
      document.removeEventListener("visibilitychange", visibilityChanged);
    };
  }, [call, onMetrics, onRecovery, onError]);
}
