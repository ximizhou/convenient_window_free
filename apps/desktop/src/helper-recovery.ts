export type HelperRecoveryDecision = "restart" | "fail";

export class HelperRecoveryGuard {
  private restartUsed = false;

  requestRecovery(): HelperRecoveryDecision {
    if (this.restartUsed) return "fail";
    this.restartUsed = true;
    return "restart";
  }

  markStable(): void {
    this.restartUsed = false;
  }

  reset(): void {
    this.restartUsed = false;
  }
}
