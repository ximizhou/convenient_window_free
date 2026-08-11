import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import MonitorStage from "./MonitorStage.svelte";
import type { DisplayInfo, HotzoneSetting } from "./types";

const display: DisplayInfo = {
  id: "primary",
  primary: true,
  bounds: { left: 0, top: 0, right: 1920, bottom: 1080 },
  workArea: { left: 0, top: 0, right: 1920, bottom: 1040 }
};

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
});
