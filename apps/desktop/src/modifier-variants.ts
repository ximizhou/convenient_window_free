import type { ModifierAction, ModifierKey } from "./types";

export const MAX_MODIFIER_VARIANTS = 15;
const modifierOrder: ModifierKey[] = ["ctrl", "alt", "shift", "win"];

export type AddModifierVariantResult =
  | { ok: true; variants: ModifierAction[]; modifiers: ModifierKey[] }
  | { ok: false; variants: ModifierAction[]; error: ModifierVariantErrorCode };

export type ModifierVariantErrorCode = "variantNeedsModifier" | "variantDuplicate" | "variantLimit";

export function addModifierVariant(
  variants: ModifierAction[] | undefined,
  modifiers: ModifierKey[]
): AddModifierVariantResult {
  const existing = variants ?? [];
  const normalized = modifierOrder.filter((modifier) => modifiers.includes(modifier));
  if (!normalized.length) {
    return { ok: false, variants: existing, error: "variantNeedsModifier" };
  }
  if (existing.some((variant) => sameModifiers(variant.modifiers, normalized))) {
    return { ok: false, variants: existing, error: "variantDuplicate" };
  }
  if (existing.length >= MAX_MODIFIER_VARIANTS) {
    return { ok: false, variants: existing, error: "variantLimit" };
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
