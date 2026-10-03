export interface NumberSettingOptions {
  value: number;
  key?: string;
  onChange(value: number): void;
}

// Keep unfinished input out of the settings snapshot; save every valid integer immediately.
export function numberSetting(node: HTMLInputElement, options: NumberSettingOptions) {
  let committed = options.value;
  let key = options.key;
  node.value = String(committed);
  node.title = `${node.min} ~ ${node.max}`;

  function publish(value: number): void {
    if (value === committed) return;
    committed = value;
    options.onChange(value);
  }

  function input(): void {
    const value = node.valueAsNumber;
    const valid = Number.isInteger(value) && node.validity.valid;
    node.setAttribute("aria-invalid", String(!valid));
    if (valid) publish(value);
  }

  function finish(): void {
    const value = node.valueAsNumber;
    const min = node.min === "" ? -Infinity : Number(node.min);
    const max = node.max === "" ? Infinity : Number(node.max);
    const next = Number.isFinite(value)
      ? Math.min(max, Math.max(min, Math.trunc(value)))
      : committed;
    node.value = String(next);
    node.setAttribute("aria-invalid", "false");
    publish(next);
  }

  function keydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      finish();
    } else if (event.key === "Escape") {
      node.value = String(committed);
      node.setAttribute("aria-invalid", "false");
    }
  }

  node.addEventListener("input", input);
  node.addEventListener("blur", finish);
  node.addEventListener("keydown", keydown);
  return {
    update(next: NumberSettingOptions): void {
      options = next;
      // Unrelated settings updates must not replace an unfinished draft.
      if (next.value !== committed || next.key !== key) {
        key = next.key;
        committed = next.value;
        node.value = String(committed);
        node.setAttribute("aria-invalid", "false");
      }
    },
    destroy(): void {
      node.removeEventListener("input", input);
      node.removeEventListener("blur", finish);
      node.removeEventListener("keydown", keydown);
    }
  };
}
