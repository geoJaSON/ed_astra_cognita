// Distances and directions on a planet's surface, from Status.json latitude/longitude.

export type LatLon = [number, number];

const rad = (d: number) => (d * Math.PI) / 180;
const deg = (r: number) => (r * 180) / Math.PI;

/** Great-circle distance in metres. */
export function surfaceDistanceM(a: LatLon, b: LatLon, radiusM: number): number {
  const dLat = rad(b[0] - a[0]);
  const dLon = rad(b[1] - a[1]);
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(rad(a[0])) * Math.cos(rad(b[0])) * Math.sin(dLon / 2) ** 2;
  return 2 * radiusM * Math.asin(Math.min(1, Math.sqrt(h)));
}

/** Initial compass bearing from `a` to `b`, degrees clockwise from north. */
export function bearingDeg(a: LatLon, b: LatLon): number {
  const dLon = rad(b[1] - a[1]);
  const y = Math.sin(dLon) * Math.cos(rad(b[0]));
  const x = Math.cos(rad(a[0])) * Math.sin(rad(b[0])) - Math.sin(rad(a[0])) * Math.cos(rad(b[0])) * Math.cos(dLon);
  return (deg(Math.atan2(y, x)) + 360) % 360;
}

export function formatDistance(m: number): string {
  return m >= 1000 ? `${(m / 1000).toFixed(m >= 10_000 ? 0 : 1)} km` : `${Math.round(m)} m`;
}

/** Approximate screen colours for the game's colour names, for swatches. */
export const COLOR_SWATCH: Record<string, string> = {
  Amethyst: "#a57ee0",
  Aquamarine: "#7fffd4",
  Blue: "#4a7cf0",
  Cobalt: "#2f63d6",
  Cyan: "#00d0e8",
  Emerald: "#3fcf7f",
  Gold: "#ffc93c",
  Green: "#4cc04c",
  Grey: "#a0a4a8",
  Indigo: "#6a55e0",
  Lime: "#b0e83a",
  Magenta: "#e84aa8",
  Maroon: "#b0344f",
  Mauve: "#c89ddc",
  Mulberry: "#c9579a",
  Ocher: "#d0892e",
  Orange: "#ff8c1a",
  Peach: "#ffb892",
  Red: "#e84848",
  Sage: "#a4b890",
  Teal: "#23b0a6",
  Turquoise: "#46e0d0",
  White: "#f2f2f2",
  Yellow: "#f2e14c",
};
