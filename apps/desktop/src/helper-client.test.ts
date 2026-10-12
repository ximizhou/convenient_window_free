import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "./settings-store";
import { HelperClient, isSupportedHelperProtocol, isSupportedHelperSchema, SUPPORTED_HELPER_PROTOCOL, SUPPORTED_HELPER_SCHEMA } from "./helper-client";

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

  it("accepts only the structured-error protocol", () => {
    expect(SUPPORTED_HELPER_PROTOCOL).toBe(7);
    expect(isSupportedHelperProtocol(7)).toBe(true);
    expect(isSupportedHelperProtocol(5)).toBe(false);
    expect(isSupportedHelperProtocol(4)).toBe(false);
  });

  it("requires the exact schema 9 number", () => {
    expect(SUPPORTED_HELPER_SCHEMA).toBe(9);
    expect(isSupportedHelperSchema(9)).toBe(true);
    for (const value of [8, 10, 9.1, "9", null, undefined, false, {}, NaN, Infinity]) {
      expect(isSupportedHelperSchema(value)).toBe(false);
    }
  });

  it.each([undefined, null, 8, 10, "9", false, {}, NaN].map(value => [value]))(
    "keeps geometry queued for schema %j until a compatible new helper connects", (schemaVersion) => {
      const client = new HelperClient();
      const settings = structuredClone(defaultSettings);
      settings.hotzones[0].geometry = { kind: "corner", width: 24, height: 48, linked: true };
      settings.hotzones[1].geometry = { kind: "edge", thickness: 16, lengthPercent: 100 };
      settings.monitorProfiles = [{ monitorId: "monitor:geometry", hotzones: structuredClone(settings.hotzones) }];
      const before = structuredClone(settings);
      expect(client.sendConfig(settings)).toBe(false);
      client.connect();
      const oldSocket = MockWebSocket.instances[0];
      oldSocket.dispatchEvent(new Event("open"));
      oldSocket.receive("helper.ready", { protocolVersion: 7, schemaVersion });
      expect(client.sendConfig(settings)).toBe(false);
      expect(oldSocket.sent).toHaveLength(0);
      expect(settings).toEqual(before);

      oldSocket.close();
      vi.advanceTimersByTime(1500);
      const newSocket = MockWebSocket.instances[1];
      newSocket.dispatchEvent(new Event("open"));
      expect(newSocket.sent).toHaveLength(0);
      newSocket.receive("helper.ready", { protocolVersion: 7, schemaVersion: 9 });
      expect(newSocket.sent).toHaveLength(1);
      expect(JSON.parse(newSocket.sent[0])).toMatchObject({ type: "config.update", data: {
        protocolVersion: 7, revision: 2, config: before
      } });
      expect(settings).toEqual(before);
    }
  );

  it("revokes config readiness on an incompatible or incomplete later handshake", () => {
    const client = new HelperClient();
    client.sendConfig(defaultSettings);
    client.connect();
    const socket = MockWebSocket.instances[0];
    socket.receive("helper.ready", { protocolVersion: 7, schemaVersion: 9 });
    expect(socket.sent).toHaveLength(1);
    for (const data of [null, {}, { protocolVersion: 7 }, { schemaVersion: 9 }, { protocolVersion: 6, schemaVersion: 9 }]) {
      socket.receive("helper.ready", data);
      expect(client.sendConfig(defaultSettings)).toBe(false);
    }
    expect(socket.sent).toHaveLength(1);
    socket.receive("helper.ready", { protocolVersion: 7, schemaVersion: 9 });
    expect(socket.sent).toHaveLength(2);
    expect(JSON.parse(socket.sent[1]).data.revision).toBe(6);
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

  it("never sends configuration to old helpers and sends translated labels only in the envelope", () => {
    let language: "zh-CN" | "en-US" = "en-US";
    const client = new HelperClient(undefined, undefined, () => language);
    client.sendConfig(defaultSettings);
    client.connect();
    const socket = MockWebSocket.instances[0];
    for (const protocolVersion of [5, 6, 8, "7", null]) {
      socket.receive("helper.ready", { protocolVersion, schemaVersion: 9 });
      expect(socket.sent).toHaveLength(0);
    }
    socket.receive("helper.ready", { protocolVersion: 7, schemaVersion: 9 });
    const envelope = JSON.parse(socket.sent[0]).data;
    expect(envelope.protocolVersion).toBe(7);
    expect(envelope.config.schemaVersion).toBe(9);
    expect(envelope.gestureLabels["gesture-up"]).toBe("Copy");
    expect(envelope.config.mouseGestures.gestures[0].name).toBeUndefined();
    language = "zh-CN";
    client.sendConfig(defaultSettings);
    expect(JSON.parse(socket.sent[1]).data.gestureLabels["gesture-up"]).toBe("复制");
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

  it("retains a validated platform capability report from helper.ready", () => {
    const client = new HelperClient();
    client.connect();
    MockWebSocket.instances[0].receive("helper.ready", {
      protocolVersion: 7,
      schemaVersion: 9,
      platform: {
        system: "linux", architecture: "x86_64", session: "x11",
        capabilities: {
          globalInput: true, windowControl: true, windowTopmost: true, screenCapture: true,
          ocr: false, audio: false, systemActions: false, edgeHide: true
        }
      }
    });
    expect(client.platformInfo).toMatchObject({ system: "linux", architecture: "x86_64" });
    expect(client.platformInfo?.capabilities.ocr).toBe(false);
    expect(client.platformInfo?.capabilities.windowTopmost).toBe(true);
  });

  it("keeps the latest config and resends it after reconnecting", () => {
    const client = new HelperClient();
    const settings = { enabled: true } as Parameters<typeof client.sendConfig>[0];

    expect(client.sendConfig(settings)).toBe(false);
    client.connect();
    MockWebSocket.instances[0].dispatchEvent(new Event("open"));
    expect(MockWebSocket.instances[0].sent).toHaveLength(0);
    MockWebSocket.instances[0].receive("helper.ready", { protocolVersion: 7, schemaVersion: 9 });
    expect(JSON.parse(MockWebSocket.instances[0].sent[0])).toMatchObject({
      type: "config.update",
      data: { revision: 1, config: { enabled: true } }
    });

    MockWebSocket.instances[0].close();
    vi.advanceTimersByTime(1500);
    MockWebSocket.instances[1].dispatchEvent(new Event("open"));
    MockWebSocket.instances[1].receive("helper.ready", { protocolVersion: 7, schemaVersion: 9 });
    expect(JSON.parse(MockWebSocket.instances[1].sent[0])).toMatchObject({
      type: "config.update",
      data: { revision: 1, config: { enabled: true } }
    });
  });

  it("increments config revisions and identifies only the latest acknowledgement", () => {
    const client = new HelperClient();
    client.connect();
    MockWebSocket.instances[0].receive("helper.ready", { protocolVersion: 7, schemaVersion: 9 });
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
    restarted.receive("helper.ready", { protocolVersion: 7, schemaVersion: 9 });

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

    socket.receive("helper.ready", { protocolVersion: 4, schemaVersion: 9 });

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
