export function needsHelperUpgrade(expectedVersion: string | null | undefined, actualVersion: unknown): boolean {
  const expected = expectedVersion?.trim();
  if (!expected) return false;
  return typeof actualVersion !== "string" || actualVersion.trim() !== expected;
}
