import { describe, expect, it } from "vitest";
import { HelperRecoveryGuard } from "./helper-recovery";

describe("HelperRecoveryGuard", () => {
  it("allows one restart and fails the next exit until stability is proven", () => {
    const guard = new HelperRecoveryGuard();

    expect(guard.requestRecovery()).toBe("restart");
    expect(guard.requestRecovery()).toBe("fail");
    expect(guard.requestRecovery()).toBe("fail");
  });

  it("re-arms only after a stable connection or an intentional reset", () => {
    const guard = new HelperRecoveryGuard();
    expect(guard.requestRecovery()).toBe("restart");
    guard.markStable();
    expect(guard.requestRecovery()).toBe("restart");
    guard.reset();
    expect(guard.requestRecovery()).toBe("restart");
  });
});
