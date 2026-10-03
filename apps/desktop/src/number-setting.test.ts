// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import nodePath from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { numberSetting } from "./number-setting";
import { defaultSettings, loadSettings, normalizeSettings, saveSettings } from "./settings-store";
import { prepareSettingsUpdate } from "./settings-sync";
import type { AppSettings } from "./types";

afterEach(() => {
  document.body.replaceChildren();
  localStorage.clear();
});

function editor(value = 700, min = 10, max = 5000, onChange = vi.fn()) {
  const node = document.createElement("input");
  node.type = "number";
  node.min = String(min);
  node.max = String(max);
  document.body.append(node);
  const action = numberSetting(node, { value, onChange });
  node.focus();
  return {
    node, action, onChange,
    type(text: string) {
      node.value = text;
      node.dispatchEvent(new Event("input", { bubbles: true }));
    },
    key(key: string) {
      node.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    }
  };
}

describe("number settings DOM editing", () => {
  it("allows 5, 50, 500 without replacing the first digit with the minimum", () => {
    const control = editor();
    control.type("5");
    expect(control.node.value).toBe("5");
    expect(control.node.getAttribute("aria-invalid")).toBe("true");
    expect(control.onChange).not.toHaveBeenCalled();
    control.type(control.node.value + "0");
    control.type(control.node.value + "0");
    expect(control.node.value).toBe("500");
    expect(control.onChange.mock.calls).toEqual([[50], [500]]);
    control.node.blur();
    expect(control.onChange).toHaveBeenCalledTimes(2);
    expect(control.node.getAttribute("aria-invalid")).toBe("false");
  });

  it.each(["", "-", "invalid"])("restores the last valid value for unfinished input %j", (text) => {
    const control = editor();
    control.type(text);
    expect(control.onChange).not.toHaveBeenCalled();
    control.node.blur();
    expect(control.node.value).toBe("700");
    expect(control.onChange).not.toHaveBeenCalled();
  });

  it.each([["3", 10], ["500", 250], ["22.8", 22]])("corrects %s visibly on blur before saving", (text, expected) => {
    const control = editor(33, 10, 250);
    control.type(text);
    expect(control.node.value).toBe(text);
    expect(control.onChange).not.toHaveBeenCalled();
    expect(control.node.title).toBe("10 ~ 250");
    control.node.blur();
    expect(control.node.value).toBe(String(expected));
    expect(control.onChange).toHaveBeenCalledExactlyOnceWith(expected);
  });

  it("supports Enter to finish and Escape to discard an invalid draft", () => {
    const control = editor(33, 10, 250);
    control.type("3");
    control.key("Enter");
    expect(control.node.value).toBe("10");
    expect(control.onChange).toHaveBeenCalledExactlyOnceWith(10);
    control.type("999");
    control.key("Escape");
    expect(control.node.value).toBe("10");
    expect(control.onChange).toHaveBeenCalledTimes(1);
  });

  it("preserves a draft across unrelated settings updates and refreshes an external value", () => {
    const control = editor();
    control.type("5");
    control.action.update({ value: 700, onChange: control.onChange });
    expect(control.node.value).toBe("5");
    control.action.update({ value: 350, onChange: control.onChange });
    expect(control.node.value).toBe("350");
    expect(control.node.getAttribute("aria-invalid")).toBe("false");
  });

  it("discards a draft when switching to another trigger with the same saved value", () => {
    const control = editor();
    control.action.update({ key: "display-1:right:hover", value: 700, onChange: control.onChange });
    control.type("5");
    const otherChange = vi.fn();
    control.action.update({ key: "display-2:left:hover", value: 700, onChange: otherChange });
    expect(control.node.value).toBe("700");
    control.node.blur();
    expect(otherChange).not.toHaveBeenCalled();
    expect(control.onChange).not.toHaveBeenCalled();
  });

  it("saves zero when allowed and keeps the last valid value after clearing", () => {
    const control = editor(300, 0, 5000);
    control.type("0");
    expect(control.onChange).toHaveBeenCalledExactlyOnceWith(0);
    control.type("");
    control.node.blur();
    expect(control.node.value).toBe("0");
    expect(control.onChange).toHaveBeenCalledTimes(1);
  });

  it("removes its event handlers when destroyed", () => {
    const control = editor();
    control.action.destroy();
    control.type("500");
    control.key("Enter");
    control.node.blur();
    expect(control.onChange).not.toHaveBeenCalled();
  });
});

const fields: [string, number, number, number][] = [
  ["hotzones.0.actions.0.hoverDelayMs", 0, 3000, 500],
  ["hotzones.0.actions.0.cooldownMs", 10, 5000, 500],
  ["edgeSize", 2, 48, 24],
  ["edgeHide.triggerDistance", 4, 96, 50],
  ["edgeHide.triggerRatio", 1, 100, 50],
  ["edgeHide.stripSize", 4, 64, 32],
  ["edgeHide.collapseDelayMs", 0, 5000, 500],
  ["edgeHide.restoreDelayMs", 0, 5000, 500],
  ["pollIntervalMs", 10, 250, 100]
];

function field(settings: AppSettings, path: string): { get(): number; set(value: number): void } {
  const keys = path.split(".");
  const leaf = keys.pop()!;
  const parent = keys.reduce((value, key) => value[key], settings as any);
  return { get: () => parent[leaf], set: (value) => { parent[leaf] = value; } };
}

describe("number settings persistence round trip", () => {
  it.each(fields)("persists the complete %s input immediately and reloads it unchanged", async (path, min, max, value) => {
    let settings = normalizeSettings(defaultSettings);
    const writes: Promise<void>[] = [];
    const onChange = vi.fn((next: number) => {
      field(settings, path).set(next);
      const prepared = prepareSettingsUpdate(settings);
      settings = prepared.editable;
      writes.push(saveSettings(prepared.normalized));
    });
    const control = editor(field(settings, path).get(), min, max, onChange);
    control.type("");
    let digits = "";
    for (const digit of String(value)) {
      digits += digit;
      control.type(digits);
      control.action.update({ value: field(settings, path).get(), onChange });
      expect(control.node.value).toBe(digits);
    }
    await Promise.all(writes);
    expect(field(loadSettings(), path).get()).toBe(value);
    expect(control.node.value).toBe(String(value));
  });

  it.each(fields)("keeps %s in sync with disk after out-of-range input is finished", async (path, min, max) => {
    let settings = normalizeSettings(defaultSettings);
    const writes: Promise<void>[] = [];
    const control = editor(field(settings, path).get(), min, max, vi.fn((next: number) => {
      field(settings, path).set(next);
      settings = prepareSettingsUpdate(settings).normalized;
      writes.push(saveSettings(settings));
    }));
    for (const [draft, expected] of [[String(min - 1), min], [String(max + 1), max]] as const) {
      control.type(draft);
      control.key("Enter");
      await Promise.all(writes);
      expect(control.node.value).toBe(String(expected));
      expect(field(loadSettings(), path).get()).toBe(expected);
    }
  });

  it("uses the draft-aware action for every number input in the settings page", () => {
    const source = readFileSync(nodePath.join(import.meta.dirname, "App.svelte"), "utf8");
    const inputs = source.match(/<input\b[^]*?\/>/g)!.filter((input) => input.includes('type="number"'));
    expect(inputs).toHaveLength(fields.length);
    for (const input of inputs) {
      expect(input).toContain("use:numberSetting=");
      expect(input).not.toContain("bind:value");
      expect(input).not.toContain("on:input");
    }
  });
});
