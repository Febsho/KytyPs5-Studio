export type IdentifiedJobEvent = { job_id: number };

export function createJobEventGate<T extends IdentifiedJobEvent>() {
  let activeId: number | null = null;
  let awaitingId = false;
  let pending: T[] = [];

  return {
    begin() {
      activeId = null;
      awaitingId = true;
      pending = [];
    },
    activate(jobId: number) {
      activeId = jobId;
      awaitingId = false;
      const queued = pending;
      pending = [];
      return queued.filter((event) => event.job_id === jobId);
    },
    receive(event: T) {
      if (awaitingId) {
        pending.push(event);
        return [];
      }
      return activeId === event.job_id ? [event] : [];
    },
    complete(jobId: number) {
      if (activeId === jobId) activeId = null;
      awaitingId = false;
      pending = [];
    },
    reset() {
      activeId = null;
      awaitingId = false;
      pending = [];
    },
  };
}
