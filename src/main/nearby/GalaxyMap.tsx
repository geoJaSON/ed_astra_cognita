import { useEffect, useImperativeHandle, useRef, useState, type Ref } from "react";
import L from "leaflet";
import "leaflet/dist/leaflet.css";
import type { Bookmark, CarrierView, Poi, Stop, Vec3 } from "../../types";
import { CATEGORY_COLORS, LANDMARKS, distance, formatLy } from "./geo";
import { regionLayers, toLatLng } from "./regions";

export interface GalaxyMapApi {
  /** Frames the given positions, or zooms to a single one. */
  frame(positions: Vec3[], maxZoom?: number): void;
  showGalaxy(): void;
}

interface Props {
  ref?: Ref<GalaxyMapApi>;
  here: Stop | null;
  route: Stop[];
  trail: Stop[];
  carrier: CarrierView | null;
  pois: Poi[];
  /** Bookmarks with a known position. */
  bookmarks: Bookmark[];
  selectedId: string | null;
  onSelect(id: string): void;
  /** Radius filter drawn around you, in ly. */
  radius: number | null;
}

// With CRS.Simple, zoom 0 is 1 px per ly; each step halves or doubles that.
const MIN_ZOOM = -8;
const MAX_ZOOM = 2;
/** Region names and borders only show when zoomed out this far. */
const REGION_OUTLINE_MAX_ZOOM = -3;
const GALAXY_BOUNDS = L.latLngBounds(L.latLng(-2000, -42000), L.latLng(66000, 40000));
/**
 * Pane drawn above the POIs, for you, landmarks, bookmarks and the selected POI. It ignores the mouse: its canvas
 * spans the whole map and would otherwise swallow clicks meant for the POI dots below. Bookmark stars are DOM
 * markers, which Leaflet makes clickable on their own.
 */
const TOP_PANE = "top";

/**
 * Tooltip content as plain text. Leaflet treats string content as HTML, and POI names come from a community feed,
 * so markup in one must never be interpreted inside the app.
 */
function text(s: string): HTMLElement {
  const el = document.createElement("span");
  el.textContent = s;
  return el;
}

/** Tooltip for the bookmarks in one system. Built from text nodes, since bookmark text is typed or imported. */
function bookmarkTooltip(list: Bookmark[], poi: Poi | undefined): HTMLElement {
  const el = document.createElement("div");
  el.className = "bm-map-tip";
  el.appendChild(document.createElement("strong")).textContent = `★ ${list[0].system}`;
  for (const b of list) {
    const line = [b.body, b.categories.join(", ")].filter(Boolean).join(" · ");
    if (line) el.appendChild(document.createElement("div")).textContent = line;
  }
  if (poi) el.appendChild(document.createElement("div")).textContent = `POI: ${poi.name}`;
  return el;
}

/** POI dot size shrinks as you zoom out so the whole galaxy stays readable. */
function poiRadius(zoom: number): number {
  if (zoom <= -5) return 2;
  if (zoom <= -3) return 3.5;
  return 5;
}

export default function GalaxyMap({ ref, here, route, trail, carrier, pois, bookmarks, selectedId, onSelect, radius }: Props) {
  const container = useRef<HTMLDivElement>(null);
  const readout = useRef<HTMLDivElement>(null);
  const map = useRef<L.Map | null>(null);
  const layers = useRef<Record<string, L.LayerGroup>>({});
  const framed = useRef(false);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;
  const hereRef = useRef(here);
  hereRef.current = here;
  const [dotRadius, setDotRadius] = useState(5);

  useImperativeHandle(ref, () => ({
    frame(positions, maxZoom = 0) {
      const m = map.current;
      if (!m || positions.length === 0) return;
      if (positions.length === 1) m.flyTo(toLatLng(positions[0]), Math.max(m.getZoom(), -2), { duration: 0.6 });
      else m.flyToBounds(L.latLngBounds(positions.map(toLatLng)).pad(0.15), { maxZoom, duration: 0.6 });
    },
    showGalaxy() {
      map.current?.flyToBounds(GALAXY_BOUNDS, { duration: 0.6 });
    },
  }));

  // Map, background and fixed landmarks, created once.
  useEffect(() => {
    const m = L.map(container.current!, {
      crs: L.CRS.Simple,
      minZoom: MIN_ZOOM,
      maxZoom: MAX_ZOOM,
      zoomSnap: 0.25,
      zoomDelta: 0.5,
      wheelPxPerZoomLevel: 90,
      preferCanvas: true,
      attributionControl: false,
      zoomControl: false,
    });
    L.control.zoom({ position: "bottomright" }).addTo(m);
    const top = m.createPane(TOP_PANE);
    top.style.zIndex = "450";
    top.style.pointerEvents = "none";
    m.fitBounds(GALAXY_BOUNDS);
    const updateDots = () => setDotRadius(poiRadius(m.getZoom()));
    m.on("zoomend", updateDots);
    updateDots();

    const { fill, borders, labels } = regionLayers();
    fill.addTo(m);
    const updateOutline = () => {
      for (const layer of [borders, labels]) {
        if (m.getZoom() <= REGION_OUTLINE_MAX_ZOOM) layer.addTo(m);
        else layer.remove();
      }
    };
    m.on("zoomend", updateOutline);
    updateOutline();

    for (const lm of LANDMARKS) {
      L.circleMarker(toLatLng(lm.pos), { pane: TOP_PANE, radius: 4, color: "#e6e6e3", weight: 1, fillOpacity: 0.9, interactive: false })
        .bindTooltip(lm.name, { permanent: true, direction: "right", className: "map-label map-label--landmark" })
        .addTo(m);
    }

    for (const name of ["trail", "route", "radius", "pois", "bookmarks", "carrier", "here"]) layers.current[name] = L.layerGroup().addTo(m);
    map.current = m;

    // Distance from you to the cursor, measured in the galactic plane (the map can't show height).
    m.on("mousemove", (e: L.LeafletMouseEvent) => {
      const h = hereRef.current;
      if (!readout.current || !h) return;
      const ly = Math.hypot(e.latlng.lng - h.pos[0], e.latlng.lat - h.pos[2]);
      readout.current.textContent = `${formatLy(ly)} from you`;
    });
    m.on("mouseout", () => {
      if (readout.current) readout.current.textContent = "";
    });

    // Resize with the panel layout.
    const observer = new ResizeObserver(() => m.invalidateSize());
    observer.observe(container.current!);
    return () => {
      observer.disconnect();
      m.remove();
      map.current = null;
    };
  }, []);

  useEffect(() => {
    const group = layers.current.trail;
    group.clearLayers();
    if (trail.length > 1) {
      L.polyline(trail.map((s) => toLatLng(s.pos)), { color: "#8d949e", weight: 1.5, opacity: 0.55, interactive: false }).addTo(group);
    }
  }, [trail]);

  useEffect(() => {
    const group = layers.current.route;
    group.clearLayers();
    if (route.length < 2) return;
    L.polyline(route.map((s) => toLatLng(s.pos)), { color: "#ff8c1a", weight: 2.5, dashArray: "6 6", interactive: false }).addTo(group);
    const end = route[route.length - 1];
    L.circleMarker(toLatLng(end.pos), { radius: 6, color: "#ff8c1a", weight: 2, fillOpacity: 0.2 })
      .bindTooltip(text(`Route end: ${end.name}`), { direction: "top", className: "map-label" })
      .addTo(group);
  }, [route]);

  useEffect(() => {
    const group = layers.current.radius;
    group.clearLayers();
    if (here && radius) {
      L.circle(toLatLng(here.pos), { radius, color: "#ff8c1a", weight: 1, opacity: 0.5, dashArray: "4 6", fill: false, interactive: false }).addTo(group);
    }
  }, [here, radius]);

  useEffect(() => {
    const group = layers.current.pois;
    group.clearLayers();
    for (const p of pois) {
      const selected = p.id === selectedId;
      L.circleMarker(toLatLng(p.pos), {
        pane: selected ? TOP_PANE : "overlayPane",
        radius: selected ? Math.max(dotRadius, 5) + 3 : dotRadius,
        color: selected ? "#ffffff" : CATEGORY_COLORS[p.category],
        weight: selected ? 2 : 1,
        fillColor: CATEGORY_COLORS[p.category],
        fillOpacity: 0.85,
      })
        .bindTooltip(() => text(p.name), { direction: "top", className: "map-label" })
        .on("click", () => onSelectRef.current(p.id))
        .addTo(group);
    }
  }, [pois, selectedId, dotRadius]);

  // One star per system, drawn as a badge up and to the right of the point so a POI's dot or your own marker
  // underneath stays visible. When the system has a POI, clicking the star selects it.
  useEffect(() => {
    const group = layers.current.bookmarks;
    group.clearLayers();
    const bySystem = new Map<string, Bookmark[]>();
    for (const b of bookmarks) {
      if (!b.pos) continue;
      const key = b.system.toLowerCase();
      bySystem.set(key, [...(bySystem.get(key) ?? []), b]);
    }
    if (bySystem.size === 0) return;
    const poiBySystem = new Map<string, Poi>();
    for (const p of pois) if (!poiBySystem.has(p.system.toLowerCase())) poiBySystem.set(p.system.toLowerCase(), p);
    const badge = { html: "★", iconSize: [16, 16], iconAnchor: [-1, 17], tooltipAnchor: [9, -17] } satisfies L.DivIconOptions;
    const icon = L.divIcon({ ...badge, className: "bm-map-icon" });
    const staticIcon = L.divIcon({ ...badge, className: "bm-map-icon bm-map-icon--static" });
    for (const [key, list] of bySystem) {
      const poi = poiBySystem.get(key);
      // Marker z-index follows the pixel y and can go negative, which would draw the star under this pane's canvas;
      // the offset keeps it on top.
      const marker = L.marker(toLatLng(list[0].pos!), { icon: poi ? icon : staticIcon, pane: TOP_PANE, keyboard: false, zIndexOffset: 1_000_000 })
        .bindTooltip(bookmarkTooltip(list, poi), { direction: "top", className: "map-label" })
        .addTo(group);
      if (poi) marker.on("click", () => onSelectRef.current(poi.id));
    }
  }, [bookmarks, pois]);

  useEffect(() => {
    const group = layers.current.carrier;
    group.clearLayers();
    if (!carrier) return;
    const icon = L.divIcon({ className: "carrier-icon", html: "FC", iconSize: [22, 16] });
    const { location, pendingJump } = carrier;
    const approx = (u: number | null) => (u ? ` (±${Math.round(u / 2)} ly)` : "");
    L.marker(toLatLng(location.pos), { icon, keyboard: false, zIndexOffset: 1000 })
      .bindTooltip(text(`Your carrier: ${location.name}${approx(location.uncertaintyLy)}`), { direction: "top", className: "map-label" })
      .addTo(group);
    if (pendingJump) {
      L.polyline([toLatLng(location.pos), toLatLng(pendingJump.pos)], { color: "#4cc9f0", weight: 1.5, dashArray: "2 5", interactive: false }).addTo(group);
      L.marker(toLatLng(pendingJump.pos), { icon: L.divIcon({ className: "carrier-icon carrier-icon--pending", html: "FC", iconSize: [22, 16] }) })
        .bindTooltip(
          text(`Carrier jumping to ${pendingJump.name}${approx(pendingJump.uncertaintyLy)} · ${formatLy(distance(location.pos, pendingJump.pos))}`),
          { direction: "top", className: "map-label" },
        )
        .addTo(group);
    }
  }, [carrier]);

  useEffect(() => {
    const group = layers.current.here;
    group.clearLayers();
    if (!here) return;
    L.circleMarker(toLatLng(here.pos), { pane: TOP_PANE, radius: 12, color: "#ff8c1a", weight: 1, fillOpacity: 0.12, interactive: false }).addTo(group);
    // The top pane ignores the mouse, so the label is permanent rather than on hover.
    L.circleMarker(toLatLng(here.pos), { pane: TOP_PANE, radius: 5, color: "#ffffff", weight: 2, fillColor: "#ff8c1a", fillOpacity: 1, interactive: false })
      .bindTooltip(text("You"), { permanent: true, direction: "right", offset: [8, 0], className: "map-label map-label--landmark" })
      .addTo(group);
  }, [here]);

  // First time we know where you are, frame you and the nearest few POIs.
  useEffect(() => {
    const m = map.current;
    if (!m || !here || framed.current || pois.length === 0) return;
    framed.current = true;
    const nearest = [...pois].sort((a, b) => distance(a.pos, here.pos) - distance(b.pos, here.pos)).slice(0, 8);
    m.fitBounds(L.latLngBounds([here.pos, ...nearest.map((p) => p.pos)].map(toLatLng)).pad(0.2), { maxZoom: -1 });
  }, [here, pois]);

  return (
    <>
      <div ref={container} className="galaxy-map" />
      <div ref={readout} className="map-readout" />
    </>
  );
}
