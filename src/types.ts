// Mirrors the serialized structs in src-tauri/src/system.rs and lib.rs.

export type BodyKind = "star" | "planet";

export interface Hotspot {
  kind: string;
  count: number;
}

export interface Ring {
  name: string;
  shortName: string;
  ringClass: string;
  /** null until the ring has been mapped with the DSS. */
  hotspots: Hotspot[] | null;
}

export interface SpeciesProgress {
  name: string;
  /** 0 = progress lost, 1 = logged, 2 = sampled, 3 = analysed. */
  samples: number;
}

export interface BioCandidate {
  id: string;
  name: string;
  /** Payout, including the first-logged bonus when it applies. */
  value: number;
  colors: string[];
}

export interface BioGenus {
  name: string;
  /** Minimum spacing between samples of one colony. */
  colonyDistanceM: number | null;
  /** Reported by the DSS or seen while sampling; otherwise only predicted. */
  confirmed: boolean;
  /** Species that could be here, cheapest first. */
  candidates: BioCandidate[];
  sampled: { species: string; samples: number } | null;
}

export interface BioView {
  signals: number;
  /** True once the DSS has reported which genera are present. */
  generaKnown: boolean;
  genera: BioGenus[];
  valueMin: number;
  valueMax: number;
  /** False when a signal couldn't be matched to any predicted species, so the range may be low. */
  valueComplete: boolean;
  /** 5 when the first-logged bonus is likely, otherwise 1. */
  multiplier: number;
}

export interface Body {
  id: number;
  name: string;
  shortName: string;
  kind: BodyKind;
  class: string;
  distanceLs: number;
  landable: boolean;
  terraformable: boolean;
  atmosphere: string | null;
  gravityG: number | null;
  temperatureK: number | null;
  wasDiscovered: boolean;
  wasMapped: boolean;
  wasFootfalled: boolean;
  mapped: boolean;
  mappedEfficiently: boolean;
  footfall: boolean;
  bioSignals: number;
  geoSignals: number;
  species: SpeciesProgress[];
  bio: BioView | null;
  rings: Ring[];
  valueScan: number;
  valueMapped: number;
}

export interface SystemView {
  name: string;
  address: number;
  starPos: [number, number, number];
  bodyCount: number | null;
  bodiesFound: number;
  allFound: boolean;
  bodies: Body[];
}

export interface OverlayState {
  enabled: boolean;
  forceShow: boolean;
  unlocked: boolean;
  visible: boolean;
}

export interface Snapshot {
  journalDir: string | null;
  commander: string | null;
  system: SystemView | null;
  gameFocused: boolean;
  guiFocus: number;
  overlay: OverlayState;
  /** Planets mapped below this value aren't highlighted. */
  worthMappingMin: number;
  /** Bodies whose best-case exobiology payout is below this aren't highlighted. */
  worthBioMin: number;
  /** The route plotted in the galaxy map; empty when none. */
  route: Stop[];
  carrier: CarrierView | null;
  /** Laden jump range of the current ship. */
  jumpRange: number | null;
  /** Systems in the travel history; re-fetch the trail when this changes. */
  historyLen: number;
  poiStatus: PoiStatus;
  /** Bumped on every bookmark change, including background position lookups; re-fetch the list when this changes. */
  bookmarksRev: number;
  /** A problem with the bookmarks file: it couldn't be read at startup, or the latest save failed. A full message. */
  bookmarksError: string | null;
  /** Why EDSM lookups of bookmark positions are failing; the app keeps retrying, and this clears once they work. */
  bookmarksLookupError: string | null;
  /** Full name of the body the game reports you're near, if any. */
  currentBody: string | null;
  /** Where you are on or over a planet. Also pushed alone as a `position` event while you move. */
  position: Surface | null;
  /** The species you're part way through sampling. */
  sampling: SamplingTrail | null;
  unsold: Unsold;
}

/** Mirrors journal::Surface, from Status.json. */
export interface Surface {
  /** Full body name. */
  body: string;
  lat: number;
  lon: number;
  /** Degrees clockwise from north. */
  heading: number | null;
  altitude: number | null;
  planetRadiusM: number | null;
  /** Landed, in the SRV or on foot, as opposed to flying over the planet. */
  onSurface: boolean;
}

/** Mirrors sampling::Trail. */
export interface SamplingTrail {
  systemAddress: number;
  bodyId: number;
  speciesId: string;
  species: string;
  colonyDistanceM: number | null;
  /** [latitude, longitude] of each sample so far, oldest first; null where the app didn't see where it was taken. */
  samples: ([number, number] | null)[];
  updated: string;
}

/** Mirrors unsold::UnsoldView: carried data that dying would lose. Estimates. */
export interface Unsold {
  cartoValue: number;
  cartoSystems: number;
  cartoBodies: number;
  bioValue: number;
  bioSpecies: number;
}

// ---- Nearby ----

export type Vec3 = [number, number, number];

export interface Stop {
  name: string;
  address: number;
  pos: Vec3;
}

export interface Place extends Stop {
  /** Edge of the sector cube the system is in, when the position is only estimated. */
  uncertaintyLy: number | null;
}

export interface CarrierView {
  location: Place;
  pendingJump: Place | null;
}

export interface PoiStatus {
  count: number;
  /** Unix seconds; 0 when nothing has loaded. */
  fetchedAt: number;
  error: string | null;
  loading: boolean;
}

export type PoiCategory =
  | "Nebulae"
  | "Stellar"
  | "Planets & scenery"
  | "Organic"
  | "Mystery & xeno"
  | "Stations & outposts"
  | "Historical"
  | "Jumponium"
  | "Other";

export interface Poi {
  id: string;
  source: "GEC" | "GMP" | string;
  name: string;
  category: PoiCategory;
  kind: string;
  system: string;
  pos: Vec3;
  summary: string;
  url: string | null;
}

// ---- Bookmarks ----

export type BookmarkPosSource = "journal" | "poi" | "edsm";

/** Mirrors bookmarks::Bookmark. Kept by this app only; the game's own bookmarks can't be read or written. */
export interface Bookmark {
  id: string;
  system: string;
  /** Free text, e.g. "3 b" or a full body name. */
  body: string | null;
  categories: string[];
  notes: string;
  pos: Vec3 | null;
  /** Where the position came from; null while it's still being resolved. */
  posSource: BookmarkPosSource | null;
  /** EDSM was asked and doesn't know the system. */
  lookupFailed: boolean;
  /** Unix seconds. */
  createdAt: number;
  updatedAt: number;
}

export interface BookmarkInput {
  /** Omit to create; set to update that bookmark. */
  id?: string;
  system: string;
  body?: string | null;
  categories: string[];
  notes: string;
  /** Supplied when the position is already known (current system, a POI). */
  pos?: Vec3 | null;
  posSource?: "journal" | "poi" | null;
}

export interface ImportResult {
  added: number;
  updated: number;
  skipped: number;
}
