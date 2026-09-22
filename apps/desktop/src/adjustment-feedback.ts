export interface AdjustmentFeedback {
  interaction: number;
  sequence: number;
  kind: "volume" | "brightness";
  level: { value: number; muted: boolean; deviceName: string } | null;
  pending: boolean;
  error: string | null;
}

export function feedbackText(feedback: AdjustmentFeedback): string {
  if (feedback.error) return "调节失败";
  if (feedback.level?.muted) return "静音";
  if (feedback.level) return `${Math.round(feedback.level.value * 100)}%`;
  return feedback.pending ? "调节中" : "";
}

export interface AdjustmentSnapshot {
  revision: number;
  feedback: AdjustmentFeedback | null;
}

export class FeedbackOrder {
  private revision = -1;
  accept(snapshot: AdjustmentSnapshot): boolean {
    if (snapshot.revision <= this.revision) return false;
    this.revision = snapshot.revision;
    return true;
  }
}
