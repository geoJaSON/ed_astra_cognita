import type { PoiCategory, Vec3 } from "../../types";

export function distance(a: Vec3, b: Vec3): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
}

function distanceToSegment(p: Vec3, a: Vec3, b: Vec3): number {
  const ab: Vec3 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
  const lengthSq = ab[0] ** 2 + ab[1] ** 2 + ab[2] ** 2;
  if (lengthSq === 0) return distance(p, a);
  const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1] + (p[2] - a[2]) * ab[2]) / lengthSq));
  return distance(p, [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t]);
}

/** Shortest distance from a point to a path through the given positions. */
export function distanceToPath(p: Vec3, path: Vec3[]): number {
  if (path.length === 0) return Infinity;
  if (path.length === 1) return distance(p, path[0]);
  let best = Infinity;
  for (let i = 1; i < path.length; i++) best = Math.min(best, distanceToSegment(p, path[i - 1], path[i]));
  return best;
}

export function formatLy(ly: number): string {
  if (ly >= 10_000) return `${(ly / 1000).toFixed(1)}k ly`;
  return `${Math.round(ly).toLocaleString()} ly`;
}

/** Rough jump count at full range; ignores neutron boosts and plotting detours. */
export function formatJumps(ly: number, jumpRange: number | null): string {
  if (!jumpRange) return "";
  const jumps = Math.max(1, Math.ceil(ly / jumpRange));
  return `~${jumps.toLocaleString()} jump${jumps === 1 ? "" : "s"}`;
}

/** GMP types are camelCase ("blackHole", "planetaryNebula"); GEC categories are already readable. */
export function kindLabel(kind: string): string {
  const words = /^[a-z]+[A-Z]/.test(kind) ? kind.replace(/POI$/, "").replace(/([A-Z])/g, " $1").trim().toLowerCase() : kind;
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export const CATEGORY_COLORS: Record<PoiCategory, string> = {
  Nebulae: "#c77dff",
  Stellar: "#ffd166",
  "Planets & scenery": "#4cc9f0",
  Organic: "#5fd38d",
  "Mystery & xeno": "#ff6b6b",
  "Stations & outposts": "#f8f9fa",
  Historical: "#f4a261",
  Jumponium: "#90be6d",
  Other: "#8d949e",
};

export const CATEGORIES = Object.keys(CATEGORY_COLORS) as PoiCategory[];

/** Landmarks drawn for orientation. */
export const LANDMARKS: { name: string; pos: Vec3 }[] = [
  { name: "Sol", pos: [0, 0, 0] },
  { name: "Sagittarius A*", pos: [25.21875, -20.90625, 25899.96875] },
  { name: "Colonia", pos: [-9530.5, -910.28125, 19808.125] },
];
