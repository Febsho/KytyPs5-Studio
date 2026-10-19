import { describe, expect, it } from "vitest";
import { AsyncSemaphore } from "./AsyncSemaphore";

const turn = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("AsyncSemaphore", () => {
  it("bounds concurrent artwork requests and releases slots in order", async () => {
    const semaphore = new AsyncSemaphore(2);
    let active = 0;
    let peak = 0;
    const results = await Promise.all(
      Array.from({ length: 8 }, (_, index) =>
        semaphore.run(async () => {
          active++;
          peak = Math.max(peak, active);
          await turn();
          active--;
          return index;
        }),
      ),
    );

    expect(results).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
    expect(peak).toBe(2);
    expect(active).toBe(0);
  });

  it("releases a slot when an artwork request fails", async () => {
    const semaphore = new AsyncSemaphore(1);
    await expect(
      semaphore.run(async () => {
        throw new Error("decode failed");
      }),
    ).rejects.toThrow("decode failed");
    await expect(semaphore.run(async () => "next request")).resolves.toBe(
      "next request",
    );
  });
});
