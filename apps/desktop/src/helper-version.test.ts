import { describe, expect, it } from "vitest";
import { needsHelperUpgrade } from "./helper-version";

describe("helper version handshake", () => {
  it("accepts the matching packaged helper", () => {
    expect(needsHelperUpgrade("0.4.0", "0.4.0")).toBe(false);
  });

  it("upgrades an older or legacy helper", () => {
    expect(needsHelperUpgrade("0.4.0", "0.3.0")).toBe(true);
    expect(needsHelperUpgrade("0.4.0", undefined)).toBe(true);
    expect(needsHelperUpgrade("0.4.0", null)).toBe(true);
  });

  it("does not force an upgrade when the expected version is unavailable", () => {
    expect(needsHelperUpgrade(undefined, "0.3.0")).toBe(false);
  });
});
