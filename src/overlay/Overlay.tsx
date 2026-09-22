import { useEffect, useState } from "react";
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

export default function Overlay() {
  const snapshot = useSnapshot();
  const position = usePosition(snapshot);
  if (!snapshot?.overlay.visible) return null;
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

  // Nothing worth doing: stay out of the way, unless it's being moved and needs something to grab.
  if (items.length === 0 && !overlay.unlocked) return null;

  return (
    <div
      className={overlay.unlocked ? "ov ov--unlocked" : "ov"}
      onMouseDown={overlay.unlocked ? () => getCurrentWindow().startDragging() : undefined}
    >
      {overlay.unlocked && <div className="ov__hint">Drag to move · lock it in the main window</div>}
      {items.length === 0 ? (
        <div className="ov__empty">Nothing worth it here</div>
      ) : (
        <ul className="ov__list">
          {items.slice(0, MAX_ROWS).map((i) => (
            <HighlightRow key={i.key} item={i} expanded={i.kind === "bio" && i.body.id === here?.id} />
          ))}
        </ul>
      )}
      {items.length > MAX_ROWS && <div className="ov__more">+{items.length - MAX_ROWS} more</div>}
      {sampling && <SamplingPanel trail={trail!} position={position!} />}
      {elsewhere > 0 && <div className="ov__more">{elsewhere} more in this system</div>}
      <UnsoldLine unsold={snapshot.unsold} />
    </div>
  );
}

function HighlightRow({ item, expanded }: { item: Highlight; expanded: boolean }) {
  const { body, kind, done } = item;
  const detail = kind === "map" || expanded ? bodyLabel(body) : bioDetail(body);
  const firstTag = kind === "bio" ? (hasFirstBio(body) ? "1st logged" : "") : body.wasDiscovered ? "1st map" : "1st disc + map";

  return (
    <li className={done ? "hl hl--done" : "hl"}>
      <div className="hl__row">
        <span className={`hl__tag hl__tag--${kind}`}>{kind === "map" ? "MAP" : "BIO"}</span>
        <span className="hl__body">{body.shortName}</span>
        <span className="hl__detail">
          {detail}
          {firstTag && <span className="hl__first">{firstTag}</span>}
        </span>
        <span className="hl__status">{done ? "✓" : item.progress}</span>
        <span className="hl__value">{formatRange(item.valueMin, item.value)}</span>
      </div>
      {expanded && body.bio && <GeneraList bio={body.bio} />}
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

function GeneraList({ bio }: { bio: BioView }) {
  const { shown, more } = generaToShow(bio);
  return (
    <ul className="gl-list">
      {shown.map((g) => (
        <GenusLine key={g.name} genus={g} />
      ))}
      {more > 0 && <li className="gl-more">+{more} more possible · map with the DSS to narrow down</li>}
    </ul>
  );
}

function GenusLine({ genus: g }: { genus: BioGenus }) {
  const samples = g.sampled?.samples ?? 0;
  const analysed = samples >= 3;
  const mark = analysed ? "✓" : samples > 0 ? `${samples}/3` : g.confirmed ? "●" : "?";
  const sampled = g.sampled ? g.candidates.find((c) => c.name === g.sampled!.species) : undefined;
  const single = !g.sampled && g.candidates.length === 1 ? g.candidates[0] : undefined;
  // Most valuable first: that's the one worth looking for.
  const options = !g.sampled && g.candidates.length > 1 ? g.candidates.slice().reverse() : [];
  const value = sampled?.value ?? g.candidates[g.candidates.length - 1]?.value;
  const cls = ["gl", analysed && "gl--done", !g.confirmed && "gl--predicted"].filter(Boolean).join(" ");

  return (
    <li className={cls}>
      <span className="gl__mark">{mark}</span>
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

function SamplingPanel({ trail, position }: { trail: SamplingTrail; position: Surface }) {
  const need = trail.colonyDistanceM;
  const radius = position.planetRadiusM;
  const me: LatLon = [position.lat, position.lon];
  const rows = trail.samples.map((s) =>
    s && radius ? { distance: surfaceDistanceM(me, s, radius), bearing: bearingDeg(me, s) } : null,
  );
  const known = rows.filter((r) => r != null);
  const nearest = known.length > 0 ? Math.min(...known.map((r) => r.distance)) : null;

  let status: { text: string; tone: string } | null = null;
  if (need != null && nearest != null) {
    if (nearest < need) status = { text: `Too close · ${formatDistance(need - nearest)} to go`, tone: "bad" };
    else if (known.length === rows.length) status = { text: `Clear · take sample ${trail.samples.length + 1}`, tone: "good" };
    else status = { text: "Clear of the samples with a known position", tone: "mixed" };
  }

  return (
    <div className="sp">
      <div className="sp__head">
        <span className="hl__tag hl__tag--bio">SAMPLE</span>
        <span className="hl__body">{trail.species}</span>
        <span>{trail.samples.length}/3</span>
        {need != null && <span className="gl__muted">{need} m apart</span>}
      </div>
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
