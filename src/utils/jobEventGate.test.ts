import { describe, expect, it } from "vitest";
import { createJobEventGate } from "./jobEventGate";

describe("job event identity gate", () => {
  it("buffers fast events until the start command returns its job ID", () => {
    const gate = createJobEventGate<{ job_id: number; line: string }>();
    gate.begin();
    const progress = { job_id: 8, line: "configure" };
    const complete = { job_id: 8, line: "done" };
    expect(gate.receive(progress)).toEqual([]);
    expect(gate.receive(complete)).toEqual([]);
    expect(gate.activate(8)).toEqual([progress, complete]);
  });

  it("drops delayed events from a previous or unrelated job", () => {
    const gate = createJobEventGate<{ job_id: number; line: string }>();
    gate.begin();
    gate.activate(12);
    expect(gate.receive({ job_id: 11, line: "old completion" })).toEqual([]);
    expect(gate.receive({ job_id: 12, line: "current output" })).toEqual([
      { job_id: 12, line: "current output" },
    ]);
    gate.complete(12);
    expect(gate.receive({ job_id: 12, line: "late output" })).toEqual([]);
  });

  it("discards queued events for a job that was not accepted", () => {
    const gate = createJobEventGate<{ job_id: number; line: string }>();
    gate.begin();
    gate.receive({ job_id: 20, line: "stale" });
    expect(gate.activate(21)).toEqual([]);
    gate.complete(21);
    gate.begin();
    expect(gate.activate(22)).toEqual([]);
  });
});
