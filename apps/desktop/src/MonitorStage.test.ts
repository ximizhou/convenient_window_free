import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import MonitorStage from "./MonitorStage.svelte";
import type { DisplayInfo, Edge, HotzoneSetting } from "./types";

const display: DisplayInfo = {
  id: "primary",
  primary: true,
  bounds: { left: 0, top: 0, right: 1920, bottom: 1080 },
  workArea: { left: 0, top: 0, right: 1920, bottom: 1040 }
};

const companionDisplay: DisplayInfo = {
  id: "secondary",
  primary: false,
  bounds: { left: 1920, top: 0, right: 3200, bottom: 1080 },
  workArea: { left: 1920, top: 0, right: 3200, bottom: 1040 }
};

function renderEdgeStage(
  edges: Edge[],
  edgeHideEnabled = true,
  english = false
): string {
  return render(MonitorStage, {
    props: {
      displays: [display],
      selectedDisplayId: display.id,
      mode: "edge-hide",
      selectedZone: "top-left",
      english,
      hotzonesEnabled: true,
      edgeHideEnabled,
      hotzones: [],
      edgeHideEdges: edges,
      displayReady: true,
      onSelectDisplay: () => undefined,
      onSelectZone: () => undefined,
      onToggleEdge: () => undefined
    }
  }).body;
}

function renderStage(
  zone: HotzoneSetting,
  selectedZone: HotzoneSetting["id"] = "top-left",
  hotzonesEnabled = true
): string {
  return render(MonitorStage, {
    props: {
      displays: [display],
      selectedDisplayId: display.id,
      mode: "hotzones",
      selectedZone,
      hotzonesEnabled,
      hotzones: [zone],
      edgeHideEdges: [],
      displayReady: true,
      onSelectDisplay: () => undefined,
      onSelectZone: () => undefined,
      onToggleEdge: () => undefined
    }
  }).body;
}

function rightZoneClass(html: string): string {
  const match = html.match(/<button[^>]*aria-label="右边缘"[^>]*class="([^"]*)"/);
  expect(match).not.toBeNull();
  return match?.[1] ?? "";
}

function rightZoneBadge(html: string): string | null {
  const match = html.match(/<button[^>]*aria-label="右边缘"[^>]*>[\s\S]*?<em[^>]*>([^<]+)<\/em>/);
  return match?.[1] ?? null;
}

function rightEdgeButton(html: string): string {
  const match = html.match(/<button[^>]*aria-label="右侧贴边隐藏"[^>]*>/);
  expect(match).not.toBeNull();
  return match?.[0] ?? "";
}

describe("MonitorStage edge-hide state", () => {
  it("renders an enabled edge bright and a disabled edge dim", () => {
    const enabled = rightEdgeButton(renderEdgeStage(["right"]));
    const disabled = rightEdgeButton(renderEdgeStage([]));

    expect(enabled).toContain('aria-pressed="true"');
    expect(enabled.match(/class="([^"]*)"/)?.[1].split(" ")).toContain("active");
    expect(disabled).toContain('aria-pressed="false"');
    expect(disabled.match(/class="([^"]*)"/)?.[1].split(" ")).not.toContain("active");
    expect(disabled.match(/class="([^"]*)"/)?.[1].split(" ")).not.toContain("configured");
  });

  it("keeps a configured edge dim while the edge-hide master switch is off", () => {
    const button = rightEdgeButton(renderEdgeStage(["right"], false));
    const classes = button.match(/class="([^"]*)"/)?.[1].split(" ") ?? [];

    expect(button).toContain('aria-pressed="true"');
    expect(classes).toContain("configured");
    expect(classes).not.toContain("active");
  });
});

describe("MonitorStage hotzone highlight", () => {
  const configuredAction = [{
    trigger: "hover" as const,
    action: { kind: "show-desktop" as const },
    cooldownMs: 700,
    hoverDelayMs: 350
  }];

  it("does not highlight configured zones while the global switch is off", () => {
    const html = renderStage({ id: "right", enabled: true, actions: configuredAction }, "top-left", false);

    expect(rightZoneClass(html).split(" ")).not.toContain("configured");
    expect(rightZoneBadge(html)).toBeNull();
  });

  it("highlights configured zones while the global switch is on", () => {
    const html = renderStage({ id: "right", enabled: false, actions: configuredAction });

    expect(rightZoneClass(html).split(" ")).toContain("configured");
  });

  it("keeps the selected configured zone active for blue fill feedback", () => {
    const html = renderStage({ id: "right", enabled: true, actions: configuredAction }, "right");
    const classes = rightZoneClass(html).split(" ");

    expect(classes).toContain("configured");
    expect(classes).toContain("active");
  });

  it("shows the number of enabled trigger actions instead of a zone index", () => {
    const actions = [
      ...configuredAction,
      { trigger: "left-click" as const, action: { kind: "show-desktop" as const }, cooldownMs: 700, hoverDelayMs: 350 }
    ];
    expect(rightZoneBadge(renderStage({ id: "right", enabled: true, actions }))).toBe("2");
    expect(rightZoneBadge(renderStage({ id: "right", enabled: true, actions: configuredAction }))).toBe("1");
    expect(rightZoneBadge(renderStage({ id: "right", enabled: true, actions }, "top-left", false))).toBeNull();
  });

  it("raises the selected monitor layer above a companion monitor", () => {
    const html = render(MonitorStage, {
      props: {
        displays: [display, companionDisplay],
        selectedDisplayId: display.id,
        mode: "hotzones",
        selectedZone: "top-left",
        hotzonesEnabled: true,
        hotzones: [],
        edgeHideEdges: [],
        onSelectDisplay: () => undefined,
        onSelectZone: () => undefined,
        onToggleEdge: () => undefined
      }
    }).body;

    expect(html).toMatch(/<article[^>]*class="[^"]*hotzone-layer[^"]*"/);
  });

  it("translates the primary display label without changing the Chinese layout", () => {
    const html = render(MonitorStage, {
      props: {
        displays: [display],
        selectedDisplayId: display.id,
        mode: "hotzones",
        selectedZone: "top-left",
        english: true,
        hotzonesEnabled: true,
        hotzones: [],
        edgeHideEdges: [],
        onSelectDisplay: () => undefined,
        onSelectZone: () => undefined,
        onToggleEdge: () => undefined
      }
    }).body;

    expect(html).toContain("Primary");
    expect(html).not.toContain("主显示器");
  });

  it("marks the fallback monitor as a preview until helper data arrives", () => {
    const html = render(MonitorStage, {
      props: {
        displays: [display],
        selectedDisplayId: display.id,
        mode: "hotzones",
        selectedZone: "top-left",
        hotzonesEnabled: true,
        hotzones: [],
        edgeHideEdges: [],
        displayReady: false,
        onSelectDisplay: () => undefined,
        onSelectZone: () => undefined,
        onToggleEdge: () => undefined
      }
    }).body;

    expect(html).toContain("预览");
    expect(html).not.toContain("主显示器");
    expect(html).toContain("等待后台助手提供显示器信息");
  });
});


describe("MonitorStage proportional geometry footprint", () => {
  const props = {
    displays: [{ ...display, bounds: { left: -2560, top: -200, right: 0, bottom: 1240 } }],
    selectedDisplayId: display.id, mode: "hotzones" as const, selectedZone: "right" as const,
    hotzonesEnabled: true, hotzones: [], edgeHideEdges: [], edgeSize: 12,
    onSelectDisplay: () => undefined, onSelectZone: () => undefined, onToggleEdge: () => undefined
  };

  it("uses absolute negative-coordinate bounds and keeps the full edge below all eight selectors", () => {
    const html = render(MonitorStage, { props: { ...props, hotzones: [{ id: "right", enabled: true, actions: [], geometry: { kind: "edge", thickness: 32, lengthPercent: 100 } }] } }).body;
    expect(html).toContain('data-rect="-32,-200,0,1240"');
    expect(html).toContain('width:1.25%;height:100%');
    expect((html.match(/class="zone zone-/g) ?? []).length).toBe(8);
  });

  it("scales rectangular corners independently rather than showing a fixed square", () => {
    const html = render(MonitorStage, { props: { ...props, selectedZone: "bottom-left", hotzones: [{ id: "bottom-left", enabled: true, actions: [], geometry: { kind: "corner", width: 128, height: 72, linked: false } }] } }).body;
    expect(html).toContain('data-rect="-2560,1168,-2432,1240"');
    expect(html).toContain('left:0%;top:95%;width:5%;height:5%');
  });

  it("falls back to legacy size and 40% without adding geometry, and hides placeholder footprints", () => {
    const html = render(MonitorStage, { props }).body;
    expect(html).toContain('data-rect="-12,232,0,808"');
    const preview = render(MonitorStage, { props: { ...props, displayReady: false } }).body;
    expect(preview).not.toContain('data-rect=');
  });
});
