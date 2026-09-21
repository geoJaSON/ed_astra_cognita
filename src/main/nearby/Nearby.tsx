import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Bookmark, Poi, PoiCategory, Snapshot, Stop } from "../../types";
import { bookmarksFor, type OpenEditor } from "../bookmarks/useBookmarks";
import CopyButton from "../CopyButton";
import GalaxyMap, { type GalaxyMapApi } from "./GalaxyMap";
import { CATEGORIES, CATEGORY_COLORS, LANDMARKS, distance, distanceToPath, formatJumps, formatLy, kindLabel } from "./geo";
import { regionAt } from "./regions";
import "./nearby.css";

const LIST_LIMIT = 30;
const RADIUS_OPTIONS = [null, 1000, 2000, 5000, 10000] as const;
const ROUTE_CORRIDOR_OPTIONS = [250, 500, 1000, 2000] as const;

type Radius = (typeof RADIUS_OPTIONS)[number];

interface Ranked extends Poi {
  ly: number;
  fromRoute: number;
}

interface Props {
  snapshot: Snapshot;
  bookmarks: Bookmark[];
  openEditor: OpenEditor;
}

export default function Nearby({ snapshot, bookmarks, openEditor }: Props) {
  const mapApi = useRef<GalaxyMapApi>(null);
  const [pois, setPois] = useState<Poi[]>([]);
  const [trail, setTrail] = useState<Stop[]>([]);
  const [hidden, setHidden] = useState<Set<PoiCategory>>(new Set());
  const [radius, setRadius] = useState<Radius>(null);
  const [nearRoute, setNearRoute] = useState(false);
  const [corridor, setCorridor] = useState<number>(500);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showBookmarks, setShowBookmarks] = useState(true);

  const { system, carrier, jumpRange, poiStatus, historyLen } = snapshot;
  // Every snapshot carries fresh copies; keep identities stable so the map and list only redo work on real changes.
  const hereAddress = system?.address;
  const here: Stop | null = useMemo(
    () => (system ? { name: system.name, address: system.address, pos: system.starPos } : null),
    [hereAddress],
  );
  const routeKey = snapshot.route.map((s) => s.address).join(",");
  const route = useMemo(() => snapshot.route, [routeKey]);

  useEffect(() => {
    invoke<Poi[]>("get_pois").then(setPois);
  }, [poiStatus.fetchedAt, poiStatus.count]);

  useEffect(() => {
    invoke<Stop[]>("get_history").then(setTrail);
  }, [historyLen]);

  const hasRoute = route.length > 1;
  const routePath = useMemo(() => route.map((s) => s.pos), [route]);

  const visible = useMemo(() => pois.filter((p) => !hidden.has(p.category)), [pois, hidden]);
  const mapBookmarks = useMemo(() => (showBookmarks ? bookmarks.filter((b) => b.pos) : []), [bookmarks, showBookmarks]);

  const ranked: Ranked[] = useMemo(() => {
    if (!here) return [];
    return visible
      .map((p) => ({ ...p, ly: distance(p.pos, here.pos), fromRoute: hasRoute ? distanceToPath(p.pos, routePath) : Infinity }))
      .filter((p) => radius == null || p.ly <= radius)
      .filter((p) => !nearRoute || !hasRoute || p.fromRoute <= corridor)
      .sort((a, b) => a.ly - b.ly);
  }, [visible, here, radius, nearRoute, corridor, hasRoute, routePath]);

  const selected = pois.find((p) => p.id === selectedId) ?? null;

  const select = (id: string) => {
    setSelectedId(id);
    const poi = pois.find((p) => p.id === id);
    if (poi) mapApi.current?.frame([poi.pos]);
  };

  const toggleCategory = (c: PoiCategory) =>
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(c)) next.delete(c);
      else next.add(c);
      return next;
    });

  const routeLength = hasRoute ? route.slice(1).reduce((sum, s, i) => sum + distance(route[i].pos, s.pos), 0) : 0;

  return (
    <div className="nearby">
      <div className="nearby__map">
        <GalaxyMap
          ref={mapApi}
          here={here}
          route={route}
          trail={trail}
          carrier={carrier}
          pois={visible}
          bookmarks={mapBookmarks}
          selectedId={selectedId}
          onSelect={select}
          radius={radius}
        />
        <div className="map-buttons">
          <button disabled={!here} onClick={() => here && mapApi.current?.frame([here.pos])}>
            Me
          </button>
          <button disabled={!hasRoute} onClick={() => mapApi.current?.frame(routePath, -1)}>
            Route
          </button>
          <button disabled={!carrier} onClick={() => carrier && mapApi.current?.frame([carrier.location.pos])}>
            Carrier
          </button>
          <button onClick={() => mapApi.current?.showGalaxy()}>Galaxy</button>
        </div>
      </div>

      <aside className="nearby__panel">
        <section className="facts">
          {here && (
            <div className="facts__row">
              <span className="muted">Region</span>
              <span>{regionAt(here.pos) ?? "unknown"}</span>
            </div>
          )}
          {here && (
            <div className="facts__row">
              {LANDMARKS.map((l) => (
                <span key={l.name}>
                  <span className="muted">{l.name}</span> {formatLy(distance(here.pos, l.pos))}
                </span>
              ))}
            </div>
          )}
          {carrier && here && (
            <div className="facts__row">
              <span className="muted">Carrier</span>
              <span className="facts__name">{carrier.location.name}</span>
              <span>
                {formatLy(distance(here.pos, carrier.location.pos))}
                {carrier.location.uncertaintyLy ? " (approx.)" : ""}
              </span>
              {carrier.pendingJump && <span className="muted">→ {carrier.pendingJump.name}</span>}
            </div>
          )}
          {hasRoute && (
            <div className="facts__row">
              <span className="muted">Route</span>
              <span className="facts__name">{route[route.length - 1].name}</span>
              <span>
                {formatLy(routeLength)} · {route.length - 1} jumps
              </span>
            </div>
          )}
        </section>

        <section className="filters">
          <div className="chips">
            {CATEGORIES.map((c) => (
              <button
                key={c}
                className={hidden.has(c) ? "chip-toggle chip-toggle--off" : "chip-toggle"}
                onClick={() => toggleCategory(c)}
              >
                <span className="dot" style={{ background: CATEGORY_COLORS[c] }} />
                {c}
              </button>
            ))}
            <button
              className={showBookmarks ? "chip-toggle" : "chip-toggle chip-toggle--off"}
              title="Show your bookmarks on the map"
              onClick={() => setShowBookmarks((v) => !v)}
            >
              <span className="bm-star" aria-hidden>
                ★
              </span>
              Bookmarks
            </button>
          </div>
          <div className="filters__row">
            <label>
              Within
              <select value={radius ?? ""} onChange={(e) => setRadius(e.target.value ? (Number(e.target.value) as Radius) : null)}>
                {RADIUS_OPTIONS.map((r) => (
                  <option key={r ?? "any"} value={r ?? ""}>
                    {r ? formatLy(r) : "any distance"}
                  </option>
                ))}
              </select>
            </label>
            <label className={hasRoute ? undefined : "disabled"} title={hasRoute ? undefined : "Plot a route in the galaxy map"}>
              <input type="checkbox" disabled={!hasRoute} checked={nearRoute && hasRoute} onChange={(e) => setNearRoute(e.target.checked)} />
              Near route
              <select disabled={!hasRoute || !nearRoute} value={corridor} onChange={(e) => setCorridor(Number(e.target.value))}>
                {ROUTE_CORRIDOR_OPTIONS.map((c) => (
                  <option key={c} value={c}>
                    ≤ {formatLy(c)}
                  </option>
                ))}
              </select>
            </label>
          </div>
        </section>

        {selected && here && (
          <PoiDetail
            poi={selected}
            ly={distance(selected.pos, here.pos)}
            jumpRange={jumpRange}
            bookmarked={bookmarksFor(bookmarks, selected.system).length > 0}
            onBookmark={() =>
              openEditor({ prefill: { system: selected.system, pos: selected.pos, posSource: "poi", notes: selected.name } })
            }
            onClose={() => setSelectedId(null)}
          />
        )}

        <ul className="poi-list">
          {!here && <li className="muted">Waiting for your position…</li>}
          {here && ranked.length === 0 && <li className="muted">No POIs match these filters.</li>}
          {ranked.slice(0, LIST_LIMIT).map((p) => (
            <li
              key={p.id}
              className={p.id === selectedId ? "poi poi--selected" : "poi"}
              onClick={() => select(p.id)}
            >
              <span className="dot" style={{ background: CATEGORY_COLORS[p.category] }} />
              <span className="poi__name">{p.name}</span>
              <span className="poi__dist">{formatLy(p.ly)}</span>
              <span className="poi__system muted">{p.system}</span>
              <span className="poi__jumps muted">
                {nearRoute && hasRoute ? `${formatLy(p.fromRoute)} off route` : formatJumps(p.ly, jumpRange)}
              </span>
            </li>
          ))}
          {ranked.length > LIST_LIMIT && (
            <li className="muted poi-list__more">
              {ranked.length - LIST_LIMIT} more; narrow the filters or pan the map
            </li>
          )}
        </ul>

        <footer className="nearby__footer">
          <span>
            {poiStatus.loading
              ? "Downloading POIs…"
              : poiStatus.count > 0
                ? `${poiStatus.count.toLocaleString()} POIs · updated ${new Date(poiStatus.fetchedAt * 1000).toLocaleDateString()}`
                : "No POI data"}
            {poiStatus.error && <span className="error" title={poiStatus.error}> · update failed</span>}
          </span>
          <button disabled={poiStatus.loading} onClick={() => invoke("refresh_pois")}>
            Refresh
          </button>
          <span className="attribution">
            POIs: Galactic Exploration Catalog (EDAstro, CMDR Orvidius) and EDSM Galactic Mapping Project, CC BY-NC-SA 3.0
          </span>
        </footer>
      </aside>
    </div>
  );
}

interface PoiDetailProps {
  poi: Poi;
  ly: number;
  jumpRange: number | null;
  /** You already have a bookmark in this POI's system. */
  bookmarked: boolean;
  onBookmark(): void;
  onClose(): void;
}

function PoiDetail({ poi, ly, jumpRange, bookmarked, onBookmark, onClose }: PoiDetailProps) {
  return (
    <section className="poi-detail">
      <div className="poi-detail__head">
        <span className="dot" style={{ background: CATEGORY_COLORS[poi.category] }} />
        <strong>{poi.name}</strong>
        <button className="poi-detail__close" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      <div className="poi-detail__meta">
        <span>{kindLabel(poi.kind) || poi.category}</span>
        <span className="muted">{poi.source}</span>
        <span>{formatLy(ly)}</span>
        <span className="muted">{formatJumps(ly, jumpRange)}</span>
      </div>
      <div className="poi-detail__system">
        <span>
          {poi.system}
          {bookmarked && (
            <span className="bm-star bm-star--after" title="You have a bookmark in this system">
              ★
            </span>
          )}
        </span>
        <CopyButton text={poi.system} label="Copy system" />
        <button onClick={onBookmark}>Bookmark</button>
        {poi.url && <button onClick={() => openUrl(poi.url!)}>Details ↗</button>}
      </div>
      {poi.summary && <p className="poi-detail__summary">{poi.summary}</p>}
    </section>
  );
}
