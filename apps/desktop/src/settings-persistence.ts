export interface SettingsCommitResult {
  revision: number;
  latest: boolean;
  ok: boolean;
  error?: unknown;
}

export class SettingsPersistence<T> {
  private revision = 0;

  constructor(private readonly write: (value: T) => Promise<void>) {}

  async commit(value: T): Promise<SettingsCommitResult> {
    const revision = ++this.revision;
    try {
      await this.write(value);
      return { revision, latest: revision === this.revision, ok: true };
    } catch (error) {
      return { revision, latest: revision === this.revision, ok: false, error };
    }
  }
}

export class SettingsApplyController<T> {
  private readonly persistence: SettingsPersistence<T>;
  private applyTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    write: (value: T) => Promise<void>,
    private readonly apply: (value: T) => boolean,
    private readonly applyDelayMs: number
  ) {
    this.persistence = new SettingsPersistence(write);
  }

  commit(value: T): Promise<SettingsCommitResult> {
    this.cancelScheduledApply();
    return this.persistence.commit(value);
  }

  async schedule(value: T, onApplied: (sent: boolean) => void): Promise<SettingsCommitResult> {
    this.cancelScheduledApply();
    const result = await this.persistence.commit(value);
    if (!result.ok || !result.latest) return result;
    this.applyTimer = setTimeout(() => {
      this.applyTimer = null;
      onApplied(this.apply(value));
    }, this.applyDelayMs);
    return result;
  }

  async applyNow(value: T): Promise<SettingsCommitResult & { sent?: boolean }> {
    this.cancelScheduledApply();
    const result = await this.persistence.commit(value);
    if (!result.ok || !result.latest) return result;
    return { ...result, sent: this.apply(value) };
  }

  dispose(): void {
    this.cancelScheduledApply();
  }

  private cancelScheduledApply(): void {
    if (this.applyTimer !== null) clearTimeout(this.applyTimer);
    this.applyTimer = null;
  }
}
