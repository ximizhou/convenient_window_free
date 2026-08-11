import type { AppSettings, HelperMessage, HelperStatus } from "./types";
import { settingsForHelper } from "./runtime-settings";

type MessageHandler = (message: HelperMessage) => void;
type StatusHandler = (status: HelperStatus) => void;

export const SUPPORTED_HELPER_PROTOCOL = 6;
export const LEGACY_HELPER_PROTOCOL = 5;

export function isSupportedHelperProtocol(value: unknown): boolean {
  return value === LEGACY_HELPER_PROTOCOL || value === SUPPORTED_HELPER_PROTOCOL;
}

export class HelperClient {
  private socket: WebSocket | null = null;
  private latestSettings: AppSettings | null = null;
  private latestConfigRevision = 0;
  private latestConfigRequestId = "";
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private shouldReconnect = false;
  private stopRequested = false;
  private protocolReady = false;
  private generation = 0;
  private readonly messageHandlers = new Set<MessageHandler>();
  private readonly statusHandlers = new Set<StatusHandler>();

  constructor(
    private readonly url = "ws://127.0.0.1:56873",
    private readonly tokenProvider: () => string | null = () => null
  ) {}

  connect(): void {
    this.openSocket(true);
  }

  private openSocket(shouldReconnect: boolean): void {
    if (shouldReconnect) this.stopRequested = false;
    this.shouldReconnect = shouldReconnect;
    this.protocolReady = false;
    this.closeSocket();
    this.clearReconnectTimer();

    const gen = ++this.generation;
    this.emitStatus("connecting");
    const token = this.tokenProvider()?.trim();
    const socket = token ? new WebSocket(this.url, token) : new WebSocket(this.url);
    this.socket = socket;

    socket.addEventListener("open", () => {
      if (gen !== this.generation) return;
      this.emitStatus("connected");
      if (this.stopRequested) {
        this.stopRequested = false;
        this.send("helper.stop", {});
        return;
      }
    });
    socket.addEventListener("close", () => {
      if (gen !== this.generation) return;
      this.socket = null;
      this.stopRequested = false;
      this.emitStatus("disconnected");
      if (this.shouldReconnect) this.scheduleReconnect();
    });
    socket.addEventListener("error", () => {
      if (gen !== this.generation) return;
      this.generation++;
      this.socket = null;
      this.stopRequested = false;
      this.emitStatus("disconnected");
      if (this.shouldReconnect) this.scheduleReconnect();
      try {
        socket.close();
      } catch {
        // The reconnect timer above is the fallback for incomplete handshakes.
      }
    });
    socket.addEventListener("message", (event) => {
      if (gen !== this.generation) return;
      try {
        const message = JSON.parse(String(event.data)) as HelperMessage;
        if (message.type === "helper.ready") {
          const protocolVersion = (message.data as { protocolVersion?: unknown } | null)?.protocolVersion;
          this.protocolReady = isSupportedHelperProtocol(protocolVersion);
          if (this.protocolReady) this.flushLatestConfig();
        }
        this.messageHandlers.forEach((handler) => handler(message));
      } catch (error) {
        console.warn("Invalid helper message", error);
      }
    });
  }

  disconnect(): void {
    this.shouldReconnect = false;
    this.stopRequested = false;
    this.clearReconnectTimer();
    this.closeSocket();
    this.emitStatus("disconnected");
  }

  private closeSocket(): void {
    if (this.socket) {
      this.generation++;
      this.socket.close();
      this.socket = null;
    }
  }

  sendConfig(settings: AppSettings): boolean {
    this.latestSettings = settingsForHelper(settings);
    this.latestConfigRevision += 1;
    this.latestConfigRequestId = crypto.randomUUID();
    return this.flushLatestConfig();
  }

  isLatestConfigRevision(revision: unknown): boolean {
    return typeof revision === "number" && revision === this.latestConfigRevision;
  }

  ping(): boolean {
    return this.send("helper.ping", {});
  }

  requestUsage(): void {
    this.send("usage.get", {});
  }

  stop(): boolean {
    this.shouldReconnect = false;
    this.clearReconnectTimer();
    if (this.send("helper.stop", {})) return true;
    this.stopRequested = true;
    if (!this.socket || this.socket.readyState !== WebSocket.CONNECTING) this.openSocket(false);
    return true;
  }

  onMessage(handler: MessageHandler): () => void {
    this.messageHandlers.add(handler);
    return () => this.messageHandlers.delete(handler);
  }

  onStatus(handler: StatusHandler): () => void {
    this.statusHandlers.add(handler);
    return () => this.statusHandlers.delete(handler);
  }

  private send(type: string, data: unknown): boolean {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      return false;
    }

    return this.sendMessage({
      id: crypto.randomUUID(),
      type,
      time: Date.now(),
      data
    });
  }

  private sendMessage(message: HelperMessage): boolean {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) return false;
    try {
      this.socket.send(JSON.stringify(message));
      return true;
    } catch {
      return false;
    }
  }

  private emitStatus(status: HelperStatus): void {
    this.statusHandlers.forEach((handler) => handler(status));
  }

  private flushLatestConfig(): boolean {
    if (!this.latestSettings || !this.protocolReady) return false;
    return this.sendMessage({
      id: this.latestConfigRequestId,
      type: "config.update",
      time: Date.now(),
      data: {
        revision: this.latestConfigRevision,
        config: this.latestSettings
      }
    });
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) {
      return;
    }

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, 1500);
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }
}
