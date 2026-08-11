import { describe, expect, it } from "vitest";
import { addModifierVariant, MAX_MODIFIER_VARIANTS } from "./modifier-variants";
import type { ModifierAction, ModifierKey } from "./types";

function variant(modifiers: ModifierKey[]): ModifierAction {
  return { modifiers, action: { kind: "none" } };
}

describe("modifier variant drafts", () => {
  it("does not persist an empty draft", () => {
    const existing = [variant(["ctrl"])];
    const result = addModifierVariant(existing, []);

    expect(result).toMatchObject({ ok: false, variants: existing });
    expect(result.variants).toBe(existing);
  });

  it("adds a unique combination without changing existing mappings", () => {
    const existing = [variant(["ctrl"])];
    const result = addModifierVariant(existing, ["alt", "shift"]);

    expect(result).toEqual({
      ok: true,
      variants: [variant(["ctrl"]), variant(["alt", "shift"])],
      modifiers: ["alt", "shift"]
    });
    expect(existing).toEqual([variant(["ctrl"])]);
  });

  it("rejects a duplicate and preserves the original array", () => {
    const existing = [variant(["ctrl", "shift"])];
    const result = addModifierVariant(existing, ["shift", "ctrl", "shift"]);

    expect(result).toMatchObject({ ok: false, message: "这个组合已经存在" });
    expect(result.variants).toBe(existing);
  });

  it("stores newly recorded modifiers in canonical order", () => {
    const result = addModifierVariant([], ["win", "ctrl", "win"]);

    expect(result).toEqual({
      ok: true,
      variants: [variant(["ctrl", "win"])],
      modifiers: ["ctrl", "win"]
    });
  });

  it("accepts all fifteen unique combinations and still identifies a duplicate at capacity", () => {
    const combinations: ModifierKey[][] = [];
    for (let mask = 1; mask <= MAX_MODIFIER_VARIANTS; mask += 1) {
      combinations.push((["ctrl", "alt", "shift", "win"] as ModifierKey[])
        .filter((_, index) => (mask & (1 << index)) !== 0));
    }
    const existing = combinations.map(variant);
    const result = addModifierVariant(existing, ["ctrl"]);

    expect(existing).toHaveLength(15);
    expect(result).toMatchObject({ ok: false, message: "这个组合已经存在" });
    expect(result.variants).toBe(existing);
  });

  it("keeps the fifteen-item capacity guard for malformed input", () => {
    const existing = Array.from({ length: MAX_MODIFIER_VARIANTS }, () => variant(["ctrl"]));
    const result = addModifierVariant(existing, ["alt"]);

    expect(result).toMatchObject({ ok: false, message: "最多可添加 15 个组合" });
    expect(result.variants).toBe(existing);
  });
});
