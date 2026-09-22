import { describe, expect, it } from "vitest";
import { feedbackText, FeedbackOrder, type AdjustmentFeedback } from "./adjustment-feedback";

describe("adjustment feedback", () => {
  const feedback: AdjustmentFeedback = {
    interaction: 1, sequence: 1, kind: "volume", pending: false, error: null,
    level: { value: 0.426, muted: false, deviceName: "Speakers" }
  };
  it("shows the readback, mute state and asynchronous failures", () => {
    expect(feedbackText(feedback)).toBe("43%");
    expect(feedbackText({ ...feedback, level: { ...feedback.level!, muted: true } })).toBe("静音");
    expect(feedbackText({ ...feedback, pending: true, level: null })).toBe("调节中");
    expect(feedbackText({ ...feedback, pending: true })).toBe("43%");
    expect(feedbackText({ ...feedback, pending: true, level: { ...feedback.level!, muted: true } })).toBe("静音");
    expect(feedbackText({ ...feedback, error: "Disconnected" })).toBe("调节失败");
  });
  it("rejects late snapshots and accepts a new helper session after reset", () => {
    const order = new FeedbackOrder();
    expect(order.accept({ revision: 2, feedback })).toBe(true);
    expect(order.accept({ revision: 1, feedback: { ...feedback, pending: true } })).toBe(false);
    expect(order.accept({ revision: 2, feedback })).toBe(false);
    expect(order.accept({ revision: 3, feedback: null })).toBe(true);
    expect(order.accept({ revision: 4, feedback: { ...feedback, sequence: 1 } })).toBe(true);
  });
});
