import { describe, expect, it, vi } from "vitest";
import { SettingsApplyController, SettingsPersistence } from "./settings-persistence";

function deferred(): { promise: Promise<void>; resolve(): void; reject(error: unknown): void } {
  let resolve!: () => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<void>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}

describe("SettingsPersistence", () => {
  it("marks only the newest completed commit as current", async () => {
    const first = deferred();
    const second = deferred();
    const write = vi.fn()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const persistence = new SettingsPersistence<number>(write);

    const firstResult = persistence.commit(1);
    const secondResult = persistence.commit(2);
    first.resolve();
    second.resolve();

    await expect(firstResult).resolves.toMatchObject({ revision: 1, latest: false, ok: true });
    await expect(secondResult).resolves.toMatchObject({ revision: 2, latest: true, ok: true });
  });

  it("returns a current persistence failure instead of hiding it", async () => {
    const persistence = new SettingsPersistence<number>(async () => {
      throw new Error("disk full");
    });

    const result = await persistence.commit(1);

    expect(result).toMatchObject({ revision: 1, latest: true, ok: false });
    expect(result.error).toEqual(new Error("disk full"));
  });
});

describe("SettingsApplyController", () => {
  it("applies only the newest durably saved snapshot after the debounce", async () => {
    vi.useFakeTimers();
    const first = deferred();
    const second = deferred();
    const write = vi.fn()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const apply = vi.fn(() => true);
    const onApplied = vi.fn();
    const controller = new SettingsApplyController(write, apply, 260);

    const firstResult = controller.schedule(1, onApplied);
    const secondResult = controller.schedule(2, onApplied);
    first.resolve();
    second.resolve();
    await firstResult;
    await secondResult;
    await vi.advanceTimersByTimeAsync(260);

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith(2);
    expect(onApplied).toHaveBeenCalledWith(true);
    controller.dispose();
    vi.useRealTimers();
  });

  it("does not apply a snapshot that failed durable persistence", async () => {
    const apply = vi.fn(() => true);
    const controller = new SettingsApplyController<number>(
      async () => { throw new Error("disk full"); },
      apply,
      260
    );

    const result = await controller.applyNow(1);

    expect(result).toMatchObject({ latest: true, ok: false });
    expect(apply).not.toHaveBeenCalled();
  });

  it("cancels a scheduled apply when an immediate commit supersedes it", async () => {
    vi.useFakeTimers();
    const apply = vi.fn(() => true);
    const controller = new SettingsApplyController<number>(async () => undefined, apply, 260);
    await controller.schedule(1, vi.fn());

    const result = await controller.applyNow(2);
    await vi.advanceTimersByTimeAsync(260);

    expect(result).toMatchObject({ latest: true, ok: true, sent: true });
    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith(2);
    controller.dispose();
    vi.useRealTimers();
  });
});
