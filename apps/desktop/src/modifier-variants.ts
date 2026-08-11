import type { ModifierAction, ModifierKey } from "./types";

export const MAX_MODIFIER_VARIANTS = 15;
const modifierOrder: ModifierKey[] = ["ctrl", "alt", "shift", "win"];

export type AddModifierVariantResult =
  | { ok: true; variants: ModifierAction[]; modifiers: ModifierKey[] }
  | { ok: false; variants: ModifierAction[]; message: string };

export function addModifierVariant(
  variants: ModifierAction[] | undefined,
  modifiers: ModifierKey[]
): AddModifierVariantResult {
  const existing = variants ?? [];
  const normalized = modifierOrder.filter((modifier) => modifiers.includes(modifier));
  if (!normalized.length) {
    return { ok: false, variants: existing, message: "请先按下至少一个修饰键" };
  }
  if (existing.some((variant) => sameModifiers(variant.modifiers, normalized))) {
    return { ok: false, variants: existing, message: "这个组合已经存在" };
  }
  if (existing.length >= MAX_MODIFIER_VARIANTS) {
    return { ok: false, variants: existing, message: `最多可添加 ${MAX_MODIFIER_VARIANTS} 个组合` };
  }
  return {
    ok: true,
    variants: [...existing, { modifiers: normalized, action: { kind: "none" } }],
    modifiers: normalized
  };
}

function sameModifiers(left: ModifierKey[], right: ModifierKey[]): boolean {
  return left.length === right.length && left.every((key, index) => key === right[index]);
}
