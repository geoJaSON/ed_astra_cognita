import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSnapshot } from "../useSnapshot";
import {
  bioDetail,
  bioHighlight,
  bodyLabel,
  formatCredits,
  formatRange,
  genusLabel,
  hasFirstBio,
  highlights,
  type Highlight,
} from "../highlights";
import type { BioGenus, BioView, SamplingTrail, Snapshot, Surface, Unsold } from "../types";
import { bearingDeg, COLOR_SWATCH, formatDistance, surfaceDistanceM, type LatLon } from "./surface";
import "./overlay.css";

const MAX_ROWS = 8;
/** Predicted genera listed under the body you're at before the DSS narrows them down. */
const MAX_PREDICTED_GENERA = 4;
const MAX_CANDIDATES = 3;

/** Latest position: from the snapshot, or from the lighter `position` events sent while only it changes. */
function usePosition(snapshot: Snapshot | null): Surface | null {
  const [position, setPosition] = useState<Surface | null>(null);
  useEffect(() => setPosition(snapshot?.position ?? null), [snapshot]);
  useEffect(() => {
    const unlisten = listen<Surface | null>("position", (e) => setPosition(e.payload));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
  return position;
}

const sameName = (a: string | null | undefined, b: string | null | undefined) =>
  a != null && b != null && a.toLowerCase() === b.toLowerCase();

/** Remember the last system actually shown, including across menu/focus changes. */
function useEntrance(visible: boolean, systemKey: string) {
  const frame = useRef<HTMLDivElement>(null);
  const lastShownSystem = useRef<string | null>(null);
  useLayoutEffect(() => {
    // StrictMode may run this effect twice on the same element.
    if (!visible || !frame.current || frame.current.dataset.enter) return;
    frame.current.dataset.enter = lastShownSystem.current === systemKey ? "return" : "power";
    lastShownSystem.current = systemKey;
  }, [visible, systemKey]);
  return frame;
}

/** Celebrate a live state change once; already-complete rows stay quiet on mount. */
function useStateFlash<T extends HTMLElement>(active: boolean, dimAfter = false) {
  const element = useRef<T>(null);
  const wasActive = useRef(active);
  useLayoutEffect(() => {
    const changed = active && !wasActive.current;
    wasActive.current = active;
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (!changed || !element.current || reducedMotion.matches) return;
    const animation = element.current.animate(
      [
        { opacity: 1, boxShadow: "inset 0 0 0 1px rgba(95, 211, 141, 0.5), inset 0 0 22px rgba(95, 211, 141, 0.15)" },
        { opacity: 1, offset: 0.3, boxShadow: "inset 0 0 0 1px rgba(95, 211, 141, 0.25), inset 0 0 16px rgba(95, 211, 141, 0.08)" },
        { opacity: dimAfter ? 0.52 : 1, boxShadow: "inset 0 0 0 1px transparent, inset 0 0 0 transparent" },
      ],
      { duration: 900, easing: "ease-out" },
    );
    const cancel = () => animation.cancel();
    reducedMotion.addEventListener("change", cancel);
    return () => {
      animation.cancel();
      reducedMotion.removeEventListener("change", cancel);
    };
  }, [active, dimAfter]);
  return element;
}

export default function Overlay() {
  const snapshot = useSnapshot();
  const position = usePosition(snapshot);
  return snapshot ? <OverlayView snapshot={snapshot} position={position} /> : null;
}

function OverlayView({ snapshot, position }: { snapshot: Snapshot; position: Surface | null }) {
  const { system, overlay } = snapshot;

  const here = system?.bodies.find((b) => sameName(b.name, snapshot.currentBody));
  const all = system ? highlights(system, { mapped: snapshot.worthMappingMin, bio: snapshot.worthBioMin }) : [];
  // On the ground at a bio planet only that planet matters, whatever it's worth.
  const landedBio = here?.bio && position?.onSurface && sameName(position.body, here.name) ? bioHighlight(here) : null;
  const items = landedBio ? [landedBio] : all;
  const elsewhere = landedBio ? all.filter((i) => i.body.id !== here!.id && !i.done).length : 0;

  const trail = snapshot.sampling;
  const sampling =
    trail && system && here && position && trail.systemAddress === system.address && trail.bodyId === here.id
      ? sameName(position.body, here.name)
      : false;

  const visible = overlay.visible && (items.length > 0 || overlay.unlocked);
  const systemKey = system ? `${system.address}:${system.name}` : "no-system";
  const frame = useEntrance(visible, systemKey);

  // Nothing worth doing: stay out of the way, unless it's being moved and needs something to grab.
  if (!visible) return null;

  return (
    <div
      key={systemKey}
      ref={frame}
      className={overlay.unlocked ? "ov ov--unlocked" : "ov"}
      onMouseDown={overlay.unlocked ? () => getCurrentWindow().startDragging() : undefined}
    >
      {overlay.unlocked && <div className="ov__hint">Drag to move · lock it in the main window</div>}
      {items.length === 0 ? (
        <div className="ov__empty">Nothing worth it here</div>
      ) : (
        <ul className="ov__list">
          {items.slice(0, MAX_ROWS).map((i) => (
            <HighlightRow key={i.key} item={i} current={i.body.id === here?.id} />
          ))}
        </ul>
      )}
      {items.length > MAX_ROWS && <div className="ov__more">+{items.length - MAX_ROWS} more</div>}
      {sampling && <SamplingPanel key={`${trail!.bodyId}:${trail!.speciesId}`} trail={trail!} position={position!} />}
      {elsewhere > 0 && <div className="ov__more">{elsewhere} more in this system</div>}
      <UnsoldLine unsold={snapshot.unsold} />
    </div>
  );
}

function HighlightRow({ item, current }: { item: Highlight; current: boolean }) {
  const { body, kind, done } = item;
  const expanded = kind === "bio" && current;
  const row = useStateFlash<HTMLLIElement>(done, true);
  const detail = kind === "map" || expanded ? bodyLabel(body) : bioDetail(body);
  const firstTag = kind === "bio" ? (hasFirstBio(body) ? "1st logged" : "") : body.wasDiscovered ? "1st map" : "1st disc + map";
  const cls = ["hl", `hl--${kind}`, done && "hl--done", current && "hl--current"].filter(Boolean).join(" ");

  return (
    <li ref={row} className={cls} aria-current={current ? "location" : undefined}>
      <div className="hl__row">
        <span className={`hl__tag hl__tag--${kind}`}>{kind === "map" ? "MAP" : "BIO"}</span>
        <span className="hl__body" title={body.name}>{body.shortName}</span>
        <span className="hl__detail" title={[detail, firstTag].filter(Boolean).join(" · ")}>
          {detail}
          {firstTag && <span className="hl__first">{firstTag}</span>}
        </span>
        <span className="hl__status">{done ? "✓" : item.progress}</span>
        <span className="hl__value">{formatRange(item.valueMin, item.value)}</span>
      </div>
      {expanded && body.bio && <GeneraList bio={body.bio} bodyDone={done} />}
    </li>
  );
}

/** Genera worth listing: what the DSS found, or before that the likeliest of what the rules allow. */
function generaToShow(bio: BioView): { shown: BioGenus[]; more: number } {
  const confirmed = bio.genera.filter((g) => g.confirmed);
  if (bio.generaKnown || confirmed.length >= bio.signals) return { shown: confirmed, more: 0 };
  const predicted = bio.genera.filter((g) => !g.confirmed);
  return {
    shown: [...confirmed, ...predicted.slice(0, MAX_PREDICTED_GENERA)],
    more: Math.max(0, predicted.length - MAX_PREDICTED_GENERA),
  };
}

function GeneraList({ bio, bodyDone }: { bio: BioView; bodyDone: boolean }) {
  const { shown, more } = generaToShow(bio);
  return (
    <ul className="gl-list">
      {shown.map((g) => (
        <GenusLine key={g.name} genus={g} bodyDone={bodyDone} />
      ))}
      {more > 0 && <li className="gl-more">+{more} more possible · map with the DSS to narrow down</li>}
    </ul>
  );
}

function GenusLine({ genus: g, bodyDone }: { genus: BioGenus; bodyDone: boolean }) {
  const samples = g.sampled?.samples ?? 0;
  const analysed = samples >= 3;
  const row = useStateFlash<HTMLLIElement>(analysed && !bodyDone, true);
  const mark = g.confirmed ? "●" : "?";
  const sampled = g.sampled ? g.candidates.find((c) => c.name === g.sampled!.species) : undefined;
  const single = !g.sampled && g.candidates.length === 1 ? g.candidates[0] : undefined;
  // Most valuable first: that's the one worth looking for.
  const options = !g.sampled && g.candidates.length > 1 ? g.candidates.slice().reverse() : [];
  const value = sampled?.value ?? g.candidates[g.candidates.length - 1]?.value;
  const cls = ["gl", analysed && "gl--done", !g.confirmed && "gl--predicted"].filter(Boolean).join(" ");

  return (
    <li ref={row} className={cls}>
      <span className="gl__mark">{g.sampled ? <SampleProgress samples={samples} /> : mark}</span>
      <span className="gl__main">
        <span className="gl__name">
          {genusLabel(g)}
          {(sampled ?? single) && <Colors colors={(sampled ?? single)!.colors} />}
          {g.colonyDistanceM != null && !analysed && <span className="gl__muted"> · {g.colonyDistanceM} m</span>}
          {g.sampled && samples === 0 && <span className="gl__muted"> · progress lost</span>}
        </span>
        {options.length > 0 && (
          <span className="gl__options">
            {options.slice(0, MAX_CANDIDATES).map((c) => (
              <span key={c.id} className="gl__option">
                {c.name.replace(`${g.name} `, "")}
                <Colors colors={c.colors} />
                <span className="gl__muted"> {formatCredits(c.value)}</span>
              </span>
            ))}
            {options.length > MAX_CANDIDATES && <span className="gl__muted">+{options.length - MAX_CANDIDATES}</span>}
          </span>
        )}
      </span>
      <span className="gl__value">{value != null ? formatCredits(value) : "?"}</span>
    </li>
  );
}

function Colors({ colors }: { colors: string[] }) {
  if (colors.length === 0) return null;
  return (
    <span className="colors">
      {colors.slice(0, 2).map((c) => (
        <span key={c} className="colors__one">
          <i style={{ background: COLOR_SWATCH[c] ?? "#888" }} />
          {c}
        </span>
      ))}
      {colors.length > 2 && <span className="gl__muted">+{colors.length - 2}</span>}
    </span>
  );
}

function SampleProgress({ samples }: { samples: number }) {
  const label = `${samples} of 3 samples collected`;
  return (
    <span className="sample-progress" role="img" aria-label={label} title={label}>
      {[1, 2, 3].map((step) => (
        <span key={step} className={step <= samples ? "sample-progress__step is-filled" : "sample-progress__step"} aria-hidden="true" />
      ))}
    </span>
  );
}

function SamplingPanel({ trail, position }: { trail: SamplingTrail; position: Surface }) {
  const need = trail.colonyDistanceM;
  const radius = position.planetRadiusM;
  const me: LatLon = [position.lat, position.lon];
  const rows = trail.samples.map((s) =>
    s && radius ? { distance: surfaceDistanceM(me, s, radius), bearing: bearingDeg(me, s) } : null,
  );
  const known = rows.filter((r) => r != null);
  const nearest = known.length > 0 ? Math.min(...known.map((r) => r.distance)) : null;
  // A full bar must never imply it's safe when an earlier sample's position is unknown.
  const progress = need != null && need > 0 && nearest != null && known.length === rows.length
    ? Math.min(1, nearest / need)
    : null;

  let status: { text: string; tone: string } | null = null;
  if (need != null && nearest != null) {
    if (nearest < need) status = { text: `Too close · ${formatDistance(need - nearest)} to go`, tone: "bad" };
    else if (known.length === rows.length) status = { text: `Clear · take sample ${trail.samples.length + 1}`, tone: "good" };
    else status = { text: "Clear of the samples with a known position", tone: "mixed" };
  }
  const panel = useStateFlash<HTMLDivElement>(status?.tone === "good");

  return (
    <div ref={panel} className="sp" data-tone={status?.tone}>
      <div className="sp__head">
        <span className="hl__tag hl__tag--bio">SAMPLE</span>
        <span className="sp__species">{trail.species}</span>
        <SampleProgress samples={trail.samples.length} />
      </div>
      {need != null && <div className="sp__spacing">{need} m minimum separation</div>}
      <div className="sp__samples">
        {rows.map((r, i) => (
          <span key={i} className={need != null && r && r.distance >= need ? "sp__sample sp__sample--clear" : "sp__sample"}>
            #{i + 1}{" "}
            {r == null ? (
              <span className="gl__muted">{radius ? "position unknown" : "no planet radius"}</span>
            ) : (
              <>
                {formatDistance(r.distance)} <Direction bearing={r.bearing} heading={position.heading} />
              </>
            )}
          </span>
        ))}
      </div>
      {progress != null && (
        <div
          className="sp__distance"
          role="progressbar"
          aria-label="Distance to clear all previous samples"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(progress * 100)}
          aria-valuetext={status?.text}
        >
          <span style={{ transform: `scaleX(${progress})` }} />
        </div>
      )}
      {status && <div className={`sp__status sp__status--${status.tone}`}>{status.text}</div>}
    </div>
  );
}

/** An arrow pointing at the sample relative to where you're facing, or the compass bearing without a heading. */
function Direction({ bearing, heading }: { bearing: number; heading: number | null }) {
  if (heading == null) return <span className="gl__muted">{Math.round(bearing)}°</span>;
  return (
    <span className="sp__arrow" style={{ transform: `rotate(${bearing - heading}deg)` }}>
      ↑
    </span>
  );
}

function UnsoldLine({ unsold: u }: { unsold: Unsold }) {
  if (u.bioValue === 0 && u.cartoValue === 0) return null;
  return (
    <div className="ov__unsold">
      <span>Unsold</span>
      {u.bioValue > 0 && (
        <span>
          bio <b>{formatCredits(u.bioValue)}</b>
        </span>
      )}
      {u.cartoValue > 0 && (
        <span>
          carto <b>≈{formatCredits(u.cartoValue)}</b>
        </span>
      )}
    </div>
  );
}
