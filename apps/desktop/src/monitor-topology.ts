import type { DisplayInfo, Edge } from "./types";

const edges: Edge[] = ["left", "top", "right", "bottom"];
const adjacencyTolerance = 1;

export function unavailableEdges(display: DisplayInfo, displays: DisplayInfo[]): Edge[] {
  return edges.filter((edge) => isFullyCovered(display, edge, displays));
}

function isFullyCovered(display: DisplayInfo, edge: Edge, displays: DisplayInfo[]): boolean {
  const vertical = edge === "left" || edge === "right";
  const start = vertical ? display.bounds.top : display.bounds.left;
  const end = vertical ? display.bounds.bottom : display.bounds.right;
  const covered = displays
    .filter((other) => other.id !== display.id && touchesOutside(display, other, edge))
    .map((other) => [
      Math.max(start, vertical ? other.bounds.top : other.bounds.left),
      Math.min(end, vertical ? other.bounds.bottom : other.bounds.right)
    ] as const)
    .filter(([from, to]) => to > from)
    .sort(([a], [b]) => a - b);

  if (!covered.length || covered[0][0] > start + adjacencyTolerance) return false;
  let coveredUntil = covered[0][1];
  for (const [from, to] of covered.slice(1)) {
    if (from > coveredUntil + adjacencyTolerance) return false;
    coveredUntil = Math.max(coveredUntil, to);
  }
  return coveredUntil >= end - adjacencyTolerance;
}

function touchesOutside(display: DisplayInfo, other: DisplayInfo, edge: Edge): boolean {
  switch (edge) {
    case "left":
      return other.bounds.left < display.bounds.left
        && other.bounds.right >= display.bounds.left - adjacencyTolerance;
    case "right":
      return other.bounds.right > display.bounds.right
        && other.bounds.left <= display.bounds.right + adjacencyTolerance;
    case "top":
      return other.bounds.top < display.bounds.top
        && other.bounds.bottom >= display.bounds.top - adjacencyTolerance;
    case "bottom":
      return other.bounds.bottom > display.bounds.bottom
        && other.bounds.top <= display.bounds.bottom + adjacencyTolerance;
  }
}


// runtime.status includes the same small display list repeatedly. Keep its identity
// stable so previews do not rerender when only foreground/action status changed.
export function sameDisplaySnapshot(left: DisplayInfo[], right: DisplayInfo[]): boolean {
  return left.length === right.length && left.every((display, index) => {
    const other = right[index];
    return display.id === other.id && display.legacyId === other.legacyId
      && display.primary === other.primary && sameRect(display.bounds, other.bounds)
      && sameRect(display.workArea, other.workArea);
  });
}

function sameRect(left: DisplayInfo["bounds"], right: DisplayInfo["bounds"]): boolean {
  return left.left === right.left && left.top === right.top
    && left.right === right.right && left.bottom === right.bottom;
}
