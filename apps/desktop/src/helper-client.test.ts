import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HelperClient, isSupportedHelperProtocol, SUPPORTED_HELPER_PROTOCOL } from "./helper-client";

class MockWebSocket extends EventTarget {
  static instances: MockWebSocket[] = [];
  static CONNECTING = 0;
  static OPEN = 1;

  readyState = MockWebSocket.OPEN;
  sent: string[] = [];

  constructor(
    public readonly url: string,
    public readonly protocols?: string | string[]
  ) {
    super();
    MockWebSocket.instances.push(this);
  }

  close(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }

  send(message: string): void {
    this.sent.push(message);
  }

  receive(type: string, data: unknown): void {
    this.dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({ id: "server", type, time: Date.now(), data })
    }));
  }
}

describe("HelperClient", () => {
  const originalWebSocket = globalThis.WebSocket;

  it("accepts the generic-host protocol and the legacy uTools protocol", () => {
    expect(SUPPORTED_HELPER_PROTOCOL).toBe(6);
    expect(isSupportedHelperProtocol(6)).toBe(true);
    expect(isSupportedHelperProtocol(5)).toBe(true);
    expect(isSupportedHelperProtocol(4)).toBe(false);
  });

  beforeEach(() => {
    vi.useFakeTimers();
    MockWebSocket.instances = [];
    globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;
  });

  afterEach(() => {
    globalThis.WebSocket = originalWebSocket;
    vi.useRealTimers();
  });

  it("does not reconnect after an intentional disconnect", () => {
    const client = new HelperClient();
    client.connect();

    expect(MockWebSocket.instances).toHaveLength(1);
    client.disconnect();
    vi.advanceTimersByTime(2000);

    expect(MockWebSocket.instances).toHaveLength(1);
  });

  it("reconnects after an unexpected close", () => {
    const client = new HelperClient();
    client.connect();

    MockWebSocket.instances[0].close();
    vi.advanceTimersByTime(1500);

    expect(MockWebSocket.instances).toHaveLength(2);
  });

  it("reconnects when an error is not followed by a close event", () => {
    const client = new HelperClient();
    client.connect();

    MockWebSocket.instances[0].dispatchEvent(new Event("error"));
    vi.advanceTimersByTime(1500);

    expect(MockWebSocket.instances).toHaveLength(2);
  });

  it("uses the current helper token as a WebSocket subprotocol", () => {
    const client = new HelperClient("ws://127.0.0.1:56787", () => "abc123");
    client.connect();

    expect(MockWebSocket.instances[0].protocols).toBe("abc123");
  });

  it("keeps the latest config and resends it after reconnecting", () => {
    const client = new HelperClient();
    const settings = { enabled: true } as Parameters<typeof client.sendConfig>[0];

    expect(client.sendConfig(settings)).toBe(false);
    client.connect();
    MockWebSocket.instances[0].dispatchEvent(new Event("open"));
    expect(MockWebSocket.instances[0].sent).toHaveLength(0);
    MockWebSocket.instances[0].receive("helper.ready", { protocolVersion: 5 });
    expect(JSON.parse(MockWebSocket.instances[0].sent[0])).toMatchObject({
      type: "config.update",
      data: { revision: 1, config: { enabled: true } }
    });

    MockWebSocket.instances[0].close();
    vi.advanceTimersByTime(1500);
    MockWebSocket.instances[1].dispatchEvent(new Event("open"));
    MockWebSocket.instances[1].receive("helper.ready", { protocolVersion: 5 });
    expect(JSON.parse(MockWebSocket.instances[1].sent[0])).toMatchObject({
      type: "config.update",
      data: { revision: 1, config: { enabled: true } }
    });
  });

  it("increments config revisions and identifies only the latest acknowledgement", () => {
    const client = new HelperClient();
    client.connect();
    MockWebSocket.instances[0].receive("helper.ready", { protocolVersion: 5 });
    client.sendConfig({ enabled: true } as Parameters<typeof client.sendConfig>[0]);
    client.sendConfig({ enabled: false } as Parameters<typeof client.sendConfig>[0]);

    expect(JSON.parse(MockWebSocket.instances[0].sent[1])).toMatchObject({
      type: "config.update",
      data: { revision: 2, config: { enabled: false } }
    });
    expect(client.isLatestConfigRevision(1)).toBe(false);
    expect(client.isLatestConfigRevision(2)).toBe(true);
  });

  it("opens a one-shot connection to stop a helper during a reconnect gap", () => {
    const disconnected = new HelperClient();
    expect(disconnected.stop()).toBe(true);
    expect(MockWebSocket.instances).toHaveLength(1);
    MockWebSocket.instances[0].dispatchEvent(new Event("open"));
    expect(JSON.parse(MockWebSocket.instances[0].sent[0])).toMatchObject({
      type: "helper.stop"
    });
    MockWebSocket.instances[0].close();
    vi.advanceTimersByTime(2000);
    expect(MockWebSocket.instances).toHaveLength(1);

    const connected = new HelperClient();
    connected.connect();
    expect(connected.stop()).toBe(true);
    expect(JSON.parse(MockWebSocket.instances[1].sent[0])).toMatchObject({
      type: "helper.stop"
    });

    MockWebSocket.instances[1].close();
    vi.advanceTimersByTime(2000);
    expect(MockWebSocket.instances).toHaveLength(2);
  });

  it("queues a stop requested while the helper connection is still opening", () => {
    const client = new HelperClient();
    client.connect();
    const socket = MockWebSocket.instances[0];
    socket.readyState = 0;

    expect(client.stop()).toBe(true);
    expect(socket.sent).toHaveLength(0);

    socket.readyState = MockWebSocket.OPEN;
    socket.dispatchEvent(new Event("open"));
    expect(JSON.parse(socket.sent[0])).toMatchObject({ type: "helper.stop" });

    socket.close();
    vi.advanceTimersByTime(2000);
    expect(MockWebSocket.instances).toHaveLength(1);
  });

  it("replaces a closing socket with a one-shot stop connection", () => {
    const client = new HelperClient();
    client.connect();
    const closing = MockWebSocket.instances[0];
    closing.readyState = 2;

    expect(client.stop()).toBe(true);
    expect(MockWebSocket.instances).toHaveLength(2);
    MockWebSocket.instances[1].dispatchEvent(new Event("open"));
    expect(JSON.parse(MockWebSocket.instances[1].sent[0])).toMatchObject({ type: "helper.stop" });
  });

  it("cancels a queued stop when an intentional connection supersedes it", () => {
    const client = new HelperClient();
    client.sendConfig({ enabled: true } as Parameters<typeof client.sendConfig>[0]);

    client.stop();
    client.connect();
    const restarted = MockWebSocket.instances[1];
    restarted.dispatchEvent(new Event("open"));
    restarted.receive("helper.ready", { protocolVersion: 5 });

    expect(JSON.parse(restarted.sent[0])).toMatchObject({
      type: "config.update",
      data: { revision: 1, config: { enabled: true } }
    });
  });

  it("can request the persisted usage snapshot without changing settings", () => {
    const client = new HelperClient();
    client.connect();
    client.requestUsage();

    expect(JSON.parse(MockWebSocket.instances[0].sent[0])).toMatchObject({
      type: "usage.get",
      data: {}
    });
  });

  it("does not send configuration to an incompatible helper protocol", () => {
    const client = new HelperClient();
    client.sendConfig({ enabled: true } as Parameters<typeof client.sendConfig>[0]);
    client.connect();
    const socket = MockWebSocket.instances[0];
    socket.dispatchEvent(new Event("open"));

    socket.receive("helper.ready", { protocolVersion: 4 });

    expect(socket.sent).toHaveLength(0);
  });

  it("reports whether a connection test request was sent", () => {
    const client = new HelperClient("ws://test", () => "abc123");
    expect(client.ping()).toBe(false);
    client.connect();
    const socket = MockWebSocket.instances[0];
    socket.dispatchEvent(new Event("open"));
    expect(client.ping()).toBe(true);
    expect(JSON.parse(socket.sent.at(-1) ?? "{}")).toMatchObject({ type: "helper.ping" });
  });
});
