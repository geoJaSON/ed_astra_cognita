import type { BioGenus, Body, SystemView } from "./types";

export interface OpenFirsts {
  discovery: boolean;
  mapping: boolean;
  footfall: boolean;
}

/** Which "first" tags are still available to you on this body. */
export function openFirsts(b: Body): OpenFirsts {
  return {
    discovery: !b.wasDiscovered,
    mapping: b.kind === "planet" && !b.wasMapped,
    footfall: b.landable && !b.wasFootfalled,
  };
}

export function hasOpenFirst(b: Body): boolean {
  const f = openFirsts(b);
  return f.discovery || f.mapping || f.footfall;
}

export function speciesDone(b: Body): number {
  return b.species.filter((s) => s.samples >= 3).length;
}

export type HighlightKind = "map" | "bio";

export interface Highlight {
  key: string;
  kind: HighlightKind;
  body: Body;
  /** Best case; equal to `valueMin` once the outcome is certain. */
  value: number;
  valueMin: number;
  done: boolean;
  progress: string;
}

export interface WorthThresholds {
  /** A planet is worth a DSS trip when its mapped value (with your first bonuses) reaches this. */
  mapped: number;
  /** A planet is worth landing on when its best-case exobiology payout reaches this. */
  bio: number;
}

/** Exobiology with the first-logged bonus still available (no footfall yet, unpopulated system). */
export function hasFirstBio(b: Body): boolean {
  return b.landable && b.bio != null && b.bio.multiplier > 1;
}

export function highlights(system: SystemView, min: WorthThresholds): Highlight[] {
  const out: Highlight[] = [];
  for (const b of system.bodies) {
    const firsts = openFirsts(b);
    if (firsts.mapping && b.valueMapped >= min.mapped) {
      out.push({
        key: `map-${b.id}`,
        kind: "map",
        body: b,
        value: b.valueMapped,
        valueMin: b.valueMapped,
        done: b.mapped,
        progress: "○",
      });
    }
    if (b.bio && hasFirstBio(b) && b.bio.valueMax >= min.bio) {
      const done = speciesDone(b);
      out.push({
        key: `bio-${b.id}`,
        kind: "bio",
        body: b,
        value: b.bio.valueMax,
        valueMin: b.bio.valueMin,
        done: done >= b.bio.signals,
        progress: `${done}/${b.bio.signals}`,
      });
    }
  }
  return out.sort((a, b) => Number(a.done) - Number(b.done) || b.value - a.value);
}

export type VerdictTone = "good" | "mixed" | "bad" | "pending";

export function verdict(system: SystemView): { label: string; tone: VerdictTone } {
  const primary = system.bodies.find((b) => b.distanceLs === 0);
  if (!primary) return { label: "Scanning…", tone: "pending" };
  if (!primary.wasDiscovered) return { label: "Undiscovered", tone: "good" };
  const undiscovered = system.bodies.filter((b) => !b.wasDiscovered).length;
  return undiscovered > 0
    ? { label: `${undiscovered} undiscovered`, tone: "mixed" }
    : { label: "Already discovered", tone: "bad" };
}

export function bodiesSummary(system: SystemView): string {
  if (system.bodyCount == null) return `${system.bodiesFound} scanned · honk for count`;
  const found = `${system.bodiesFound}/${system.bodyCount} bodies`;
  return system.allFound ? `${found} · all found` : found;
}

/** "5.0–42M" style, or a single value when the range has collapsed. */
export function formatRange(min: number, max: number): string {
  if (min === max) return formatCredits(max);
  if (min >= 1e6) {
    const m = (v: number) => (v / 1e6).toFixed(v < 1e7 ? 1 : 0);
    return `${m(min)}–${m(max)}M`;
  }
  return `${formatCredits(min)}–${formatCredits(max)}`;
}

export function formatCredits(v: number): string {
  if (v >= 1e7) return `${(v / 1e6).toFixed(1)}M`;
  if (v >= 1e6) return `${(v / 1e6).toFixed(2)}M`;
  if (v >= 1e3) return `${Math.round(v / 1e3)}k`;
  return String(v);
}

const PLANET_LABELS: Record<string, string> = {
  "Earthlike body": "Earth-like world",
  "Water world": "Water world",
  "Ammonia world": "Ammonia world",
  "High metal content body": "High metal content",
  "Metal rich body": "Metal-rich",
  "Rocky body": "Rocky",
  "Icy body": "Icy",
  "Rocky ice body": "Rocky ice",
  "Sudarsky class I gas giant": "Class I gas giant",
  "Sudarsky class II gas giant": "Class II gas giant",
  "Sudarsky class III gas giant": "Class III gas giant",
  "Sudarsky class IV gas giant": "Class IV gas giant",
  "Sudarsky class V gas giant": "Class V gas giant",
  "Gas giant with water based life": "Gas giant, water life",
  "Gas giant with ammonia based life": "Gas giant, ammonia life",
  "Helium rich gas giant": "Helium-rich gas giant",
  "Helium gas giant": "Helium gas giant",
  "Water giant": "Water giant",
};

function starLabel(type: string): string {
  if (type === "N") return "Neutron star";
  if (type === "H") return "Black hole";
  if (type.startsWith("D")) return `White dwarf (${type})`;
  return `${type} star`;
}

export function bodyLabel(b: Body): string {
  if (b.kind === "star") return starLabel(b.class);
  const label = PLANET_LABELS[b.class] ?? b.class;
  return b.terraformable && b.class !== "Earthlike body" ? `Terraformable ${label.toLowerCase()}` : label;
}

/** The species name once only one is possible, otherwise the genus. */
export function genusLabel(g: BioGenus): string {
  if (g.sampled) return g.sampled.species;
  return g.candidates.length === 1 ? g.candidates[0].name : g.name;
}

/** The genus being sampled right now (logged but not yet analysed), if any. */
export function activeSampling(b: Body): BioGenus | undefined {
  return b.bio?.genera.find((g) => g.sampled && g.sampled.samples >= 1 && g.sampled.samples < 3);
}

const MAX_PREDICTED_SHOWN = 3;

/** One-line summary: what's being sampled, what the DSS found, or what might be there. */
export function bioDetail(b: Body): string {
  const bio = b.bio;
  if (!bio) return `${b.bioSignals} bio signal${b.bioSignals === 1 ? "" : "s"}`;
  const active = activeSampling(b);
  if (active?.sampled) {
    const spacing = active.colonyDistanceM ? ` · ${active.colonyDistanceM} m apart` : "";
    return `${active.sampled.species} ${active.sampled.samples}/3${spacing}`;
  }
  const confirmed = bio.genera.filter((g) => g.confirmed);
  if (bio.generaKnown || confirmed.length >= bio.signals) return confirmed.map(genusLabel).join(", ");
  const predicted = bio.genera.filter((g) => !g.confirmed);
  const shown = predicted.slice(0, MAX_PREDICTED_SHOWN).map((g) => `${genusLabel(g)}?`);
  const more = predicted.length > MAX_PREDICTED_SHOWN ? ` +${predicted.length - MAX_PREDICTED_SHOWN}` : "";
  return [...confirmed.map(genusLabel), ...shown].join(", ") + more;
}
