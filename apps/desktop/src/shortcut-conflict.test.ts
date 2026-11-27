import { readFileSync } from "node:fs";
import nodePath from "node:path";
import { render } from "svelte/server";
import ts from "typescript";
import { describe, expect, it } from "vitest";
import ShortcutRecorder from "./ShortcutRecorder.svelte";
import { format, translator } from "./i18n";
import { formatShortcut, heldModifiers, isModifierKey, nextRecordingStep, resolveKeyName } from "./shortcut-keys";

const component = readFileSync(nodePath.join(import.meta.dirname, "ShortcutRecorder.svelte"), "utf8");
const app = readFileSync(nodePath.join(import.meta.dirname, "App.svelte"), "utf8");

// 剥掉 TypeScript 类型标注：用项目自带的 typescript 编译器转译，而不是手写正则。
function stripTypes(code: string): string {
  return ts.transpileModule(code, {
    compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext }
  }).outputText;
}

type KeyState = Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>>;

function event(key: string, code: string, modifiers: KeyState = {}) {
  return {
    key,
    code,
    ctrlKey: Boolean(modifiers.ctrlKey),
    altKey: Boolean(modifiers.altKey),
    shiftKey: Boolean(modifiers.shiftKey),
    metaKey: Boolean(modifiers.metaKey)
  } as KeyboardEvent;
}

function html(props: { value: string; label: string; english: boolean }): string {
  const out = render(ShortcutRecorder, { props: { ...props, onChange: () => {} } });
  return `${out.head}${out.body}`;
}

describe("recording a full key sequence", () => {
  // 按一次完整序列（修饰键按下 → 主键），返回最后一步结果。
  function record(sequence: Array<KeyboardEvent>) {
    let step = nextRecordingStep(sequence[0]);
    for (const item of sequence.slice(1)) step = nextRecordingStep(item);
    return step;
  }

  it("records common modifier combinations", () => {
    const cases: Array<[string, KeyboardEvent[]]> = [
      ["Ctrl+C", [event("Control", "ControlLeft", { ctrlKey: true }), event("c", "KeyC", { ctrlKey: true })]],
      ["Ctrl+V", [event("Control", "ControlLeft", { ctrlKey: true }), event("v", "KeyV", { ctrlKey: true })]],
      ["Ctrl+X", [event("Control", "ControlLeft", { ctrlKey: true }), event("x", "KeyX", { ctrlKey: true })]],
      ["Ctrl+Alt+T", [
        event("Control", "ControlLeft", { ctrlKey: true }),
        event("Alt", "AltLeft", { ctrlKey: true, altKey: true }),
        event("t", "KeyT", { ctrlKey: true, altKey: true })
      ]],
      ["Ctrl+Shift+S", [
        event("Control", "ControlLeft", { ctrlKey: true }),
        event("Shift", "ShiftLeft", { ctrlKey: true, shiftKey: true }),
        event("s", "KeyS", { ctrlKey: true, shiftKey: true })
      ]],
      ["Alt+F4", [event("Alt", "AltLeft", { altKey: true }), event("F4", "F4", { altKey: true })]],
      ["Win+D", [event("Meta", "MetaLeft", { metaKey: true }), event("d", "KeyD", { metaKey: true })]],
      ["Alt+1", [event("Alt", "AltLeft", { altKey: true }), event("1", "Digit1", { altKey: true })]],
      ["Ctrl+Right", [event("Control", "ControlLeft", { ctrlKey: true }), event("ArrowRight", "ArrowRight", { ctrlKey: true })]]
    ];
    for (const [expected, sequence] of cases) {
      expect(record(sequence), `${expected} should be recordable`).toEqual({ action: "commit", value: expected });
    }
  });

  it("waits while only modifiers are held", () => {
    expect(nextRecordingStep(event("Control", "ControlLeft", { ctrlKey: true }))).toEqual({ action: "wait", hint: "Ctrl" });
    expect(nextRecordingStep(event("Shift", "ShiftLeft", { ctrlKey: true, shiftKey: true }))).toEqual({
      action: "wait",
      hint: "Ctrl+Shift"
    });
  });

  it("cancels on Escape", () => {
    expect(nextRecordingStep(event("Escape", "Escape"))).toEqual({ action: "cancel" });
  });

  it("waits instead of committing unsupported keys", () => {
    for (const [key, code] of [
      ["/", "Slash"],
      [";", "Semicolon"],
      ["[", "BracketLeft"],
      ["Backspace", "Backspace"],
      ["Delete", "Delete"],
      ["Home", "Home"],
      ["PageUp", "PageUp"]
    ] as const) {
      expect(nextRecordingStep(event(key, code, { ctrlKey: true })), `${key} must not commit`).toEqual({
        action: "wait",
        hint: ""
      });
    }
  });
});

describe("shortcut key resolution", () => {
  it("records a bare key without any modifier", () => {
    for (const [key, code, expected] of [
      ["a", "KeyA", "A"],
      ["5", "Digit5", "5"],
      ["F5", "F5", "F5"],
      ["ArrowLeft", "ArrowLeft", "Left"],
      [" ", "Space", "Space"],
      ["Enter", "Enter", "Enter"],
      ["Tab", "Tab", "Tab"]
    ] as const) {
      expect(resolveKeyName(event(key, code)), `${key} should resolve on its own`).toBe(expected);
    }
  });

  it("treats modifier keys themselves as non-recordable", () => {
    for (const key of ["Control", "Alt", "Shift", "Meta"] as const) {
      expect(isModifierKey(event(key, key))).toBe(true);
      expect(resolveKeyName(event(key, key))).toBeNull();
    }
  });

  it("only accepts F1..F12 and rejects the F13..F24 range", () => {
    // helper 的 parse_key 只映射到 VK_F12，F13..F24 录进去会在执行时报
    // unsupported shortcut key。前端此前按 F1..F24 放行，属两端键集漂移。
    for (let index = 1; index <= 12; index += 1) {
      expect(resolveKeyName(event(`F${index}`, `F${index}`)), `F${index} should be recordable`).toBe(`F${index}`);
    }
    for (let index = 13; index <= 24; index += 1) {
      expect(resolveKeyName(event(`F${index}`, `F${index}`)), `F${index} must be rejected`).toBeNull();
      expect(nextRecordingStep(event(`F${index}`, `F${index}`)), `F${index} must not commit`).toEqual({
        action: "wait",
        hint: ""
      });
    }
  });

  it("takes the physical key from the layout-independent code", () => {
    // AZERTY 等布局下 e.key 会变成布局字符，必须按 e.code 记录物理键位。
    expect(resolveKeyName(event("q", "KeyA"))).toBe("A");
    expect(resolveKeyName(event("&", "Digit1"))).toBe("1");
  });

  it("still records when the host omits e.code", () => {
    // 某些嵌入环境（webview / Electron）里 e.code 可能缺失或不带 Key/Digit 前缀。
    // 此时必须退回 e.key，否则只有 F 键能录——正是「只认特殊键位」的现象。
    const noCode = { key: "c", ctrlKey: true } as KeyboardEvent;
    expect(resolveKeyName(noCode)).toBe("C");
    expect(nextRecordingStep({ key: "Control", code: "", ctrlKey: true } as KeyboardEvent)).toEqual({
      action: "wait",
      hint: "Ctrl"
    });
    expect(nextRecordingStep(noCode)).toEqual({ action: "commit", value: "Ctrl+C" });

    // 但依然不能放行 helper 不支持的标点
    expect(resolveKeyName({ key: "/", ctrlKey: true } as KeyboardEvent)).toBeNull();
    expect(resolveKeyName({ key: "5" } as KeyboardEvent)).toBe("5");
  });

  it("orders modifiers as Ctrl, Alt, Shift, Win", () => {
    const state = { shiftKey: true, metaKey: true, altKey: true, ctrlKey: true };
    expect(heldModifiers(event("a", "KeyA", state))).toEqual(["ctrl", "alt", "shift", "win"]);
    expect(formatShortcut(heldModifiers(event("a", "KeyA", state)), "A")).toBe("Ctrl+Alt+Shift+Win+A");
    expect(formatShortcut([], "F5")).toBe("F5");
    expect(formatShortcut(["ctrl"], "Enter")).toBe("Ctrl+Enter");
  });
});

describe("shortcut recorder component", () => {
  it("renders the record hint, key caps and clear control", () => {
    const empty = html({ value: "", label: "录制快捷键", english: false });
    expect(empty).toContain("点击录制（支持单键）");
    expect(empty).not.toContain("清除快捷键");
    expect(html({ value: "", label: "Record shortcut", english: true })).toContain("Record shortcut");

    const filled = html({ value: "Ctrl+Alt+A", label: "录制快捷键", english: false });
    expect(filled).toMatch(/<kbd[^>]*>Ctrl<\/kbd>/);
    expect(filled).toMatch(/<kbd[^>]*>Alt<\/kbd>/);
    expect(filled).toMatch(/<kbd[^>]*>A<\/kbd>/);
    expect(filled).toContain("清除快捷键");
  });

  it("listens on the window so focus and browser shortcuts cannot swallow keys", () => {
    // 只监听按钮自身时，焦点不在按钮上就收不到按键；而 Ctrl+C/Ctrl+V 本身是
    // 浏览器快捷键，会被优先消费，表现成「组合键录不上」。
    // 因此挂在 window 捕获阶段，并在按钮上重复绑定一次作为兜底。
    // 捕获阶段已 stopPropagation，window 监听生效时按钮不会重复触发。
    expect(component).toContain('window.addEventListener("keydown"');
    expect(component).toContain("handleKeydown");
    expect(component).toContain(", true)");
    expect(component).toContain("on:keydown={handleKeydown}");
  });

  it("cancels with Escape and supports clearing", () => {
    expect(component).toContain("onChange(step.value)");
    expect(component).toContain('onChange("")');
  });
});

describe("shortcut output reuse", () => {
  const target: { kind: string; value?: string } = { kind: "none" };
  const settings = {
    hotzones: [{ id: "left", actions: [{ trigger: "hover", action: { kind: "shortcut", value: "Ctrl+Alt+A" } }] }],
    monitorProfiles: []
  };

  // 从 App.svelte 提取真实的 setActionShortcut 执行，测的是产品代码而不是复制品。
  function load() {
    const declaration = /function\s+setActionShortcut\b/.exec(app);
    if (!declaration || declaration.index === undefined) throw new Error("setActionShortcut not found");
    let depth = 0;
    let end = declaration.index;
    for (let i = declaration.index; i < app.length; i += 1) {
      if (app[i] === "{") depth += 1;
      else if (app[i] === "}") {
        depth -= 1;
        if (depth === 0) { end = i + 1; break; }
      }
    }
    const handler = stripTypes(app.slice(declaration.index, end));
    return new Function(
      "settings",
      "target",
      "persist",
      "ensureHotzoneActionTarget",
      "currentAction",
      "ui",
      "format",
      `let shortcutError = "";
       let shortcutDraft = "";
       let actionEditorRevision = 0;
       let selectedDisplayId = 'test';
       let selectedZone = 'top';
       let activeTrigger = 'hover';
       let selectedHotzoneModifiers = [];
       let actionEditorOverrideKey = '';
       let actionEditorOverride = null;
       const modifierId = () => '';
       ${handler}
       return {
         setActionShortcut,
         readError: () => shortcutError,
         readDraft: () => shortcutDraft,
         readRevision: () => actionEditorRevision
       };`
    )(settings, target, () => {}, () => ({ slot: {}, action: target }), () => target, translator("zh-CN"), format) as {
      setActionShortcut: (value: string) => void;
      readError: () => string;
      readDraft: () => string;
      readRevision: () => number;
    };
  }

  it("allows two independent regions to emit the same shortcut", () => {
    const instance = load();
    target.kind = "none";
    delete target.value;
    instance.setActionShortcut("Ctrl+Alt+A");
    expect(instance.readError()).toBe("");
    expect(target.value).toBe("Ctrl+Alt+A");
    expect(settings.hotzones[0].actions[0].action.value).toBe("Ctrl+Alt+A");
  });

  it("accepts a fresh combination", () => {
    const instance = load();
    target.kind = "none";
    delete target.value;
    instance.setActionShortcut("Ctrl+Shift+K");
    expect(instance.readError()).toBe("");
    expect(target.kind).toBe("shortcut");
    expect(target.value).toBe("Ctrl+Shift+K");
    // 回归防护：currentAction() 返回的对象不是响应式的，改它的属性界面刷不出来。
    // 录制结果必须同时写进 shortcutDraft，否则界面会停在空态。
    expect(instance.readDraft()).toBe("Ctrl+Shift+K");
  });

  it("turns an empty recording into a cleared action", () => {
    const instance = load();
    target.kind = "shortcut";
    target.value = "Ctrl+Alt+A";
    const revisionBefore = instance.readRevision();
    instance.setActionShortcut("");
    expect(target.kind).toBe("none");
    expect(target.value).toBeUndefined();
    expect(instance.readError()).toBe("");
    expect(instance.readDraft()).toBe("");
    // 回归防护：动作编辑区被 {#key actionEditorRevision} 包住，块内表达式被编译器整体
    // untrack，只有 key 变化才重建。不递增 revision 时，清空后下拉框仍显示"自定义快捷键"、
    // 录制器仍然存在（已用端到端复现），必须锁死这一点。
    expect(instance.readRevision()).toBe(revisionBefore + 1);
  });

  it("accepts a bare key when it is not already taken", () => {
    const instance = load();
    target.kind = "none";
    delete target.value;
    instance.setActionShortcut("F5");
    expect(instance.readError()).toBe("");
    expect(target.value).toBe("F5");
    expect(instance.readDraft()).toBe("F5");
  });
});
