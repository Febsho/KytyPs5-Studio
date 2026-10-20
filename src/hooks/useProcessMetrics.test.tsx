// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ProcessMetric } from "../types/studio";
import type { StudioCommand } from "./useStudioData";
import {
  processMetricsPollDelay,
  refreshProcessMetricsNow,
  useProcessMetrics,
} from "./useProcessMetrics";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const running = [{ status: "Running" }] as ProcessMetric[];

describe("process telemetry polling", () => {
  it("backs off when idle or hidden and stays responsive for active sessions", () => {
    expect(processMetricsPollDelay(false, true)).toBe(15_000);
    expect(processMetricsPollDelay(false, false)).toBe(60_000);
    expect(processMetricsPollDelay(true, true)).toBe(5_000);
    expect(processMetricsPollDelay(true, false)).toBe(15_000);
  });

  it("polls promptly after launch and then adapts to the active session state", async () => {
    vi.useFakeTimers();
    const call = vi.fn().mockResolvedValueOnce(running).mockResolvedValue([]);
    renderHook(() =>
      useProcessMetrics(call, vi.fn(), vi.fn(), vi.fn()),
    );

    await act(async () => {
      await Promise.resolve();
    });
    expect(call).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4_999);
    });
    expect(call).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(call).toHaveBeenCalledTimes(2);
  });

  it("supports immediate refresh requests and discards late results after unmount", async () => {
    vi.useFakeTimers();
    let resolve: ((metrics: ProcessMetric[]) => void) | undefined;
    const call = vi.fn(
      <T,>() =>
        new Promise<T>((done) => {
          resolve = (metrics) => done(metrics as T);
        }),
    );
    const onMetrics = vi.fn();
    const { unmount } = renderHook(() =>
      useProcessMetrics(call as StudioCommand, onMetrics, vi.fn(), vi.fn()),
    );
    act(() => refreshProcessMetricsNow());
    expect(call).toHaveBeenCalledTimes(1);
    unmount();
    await act(async () => {
      resolve?.(running);
      await Promise.resolve();
    });
    expect(onMetrics).not.toHaveBeenCalled();
  });
});
