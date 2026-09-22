import { useState } from "react";
import {
  bioDetail,
  bodiesSummary,
  bodyLabel,
  formatCredits,
  formatRange,
  hasFirstBio,
  hasOpenFirst,
  highlights,
  openFirsts,
  speciesDone,
  verdict,
} from "../highlights";
import type { Body, Bookmark, Snapshot, Unsold } from "../types";
import BookmarkBanner from "./bookmarks/BookmarkBanner";
import { bookmarksFor, shortBodyName, type OpenEditor } from "./bookmarks/useBookmarks";
import CopyButton from "./CopyButton";
import Exobiology from "./Exobiology";
import WorthSlider, { BIO_STEPS, MAPPED_STEPS } from "./WorthSlider";

interface Props {
  snapshot: Snapshot;
  bookmarks: Bookmark[];
  openEditor: OpenEditor;
}

export default function CurrentSystem({ snapshot, bookmarks, openEditor }: Props) {
  const [onlyOpen, setOnlyOpen] = useState(true);
  const { system } = snapshot;
  if (!system) return <p className="muted">No system yet. Jump somewhere or load the game.</p>;

  const v = verdict(system);
  const items = highlights(system, { mapped: snapshot.worthMappingMin, bio: snapshot.worthBioMin });
  const bodies = onlyOpen ? system.bodies.filter(hasOpenFirst) : system.bodies;

  const bookmarkHere = () =>
    openEditor({
      prefill: {
        system: system.name,
        body: shortBodyName(snapshot.currentBody, system.name),
        pos: system.starPos,
        posSource: "journal",
      },
    });

  return (
    <div className="system">
      <section className="system__head">
        <div>
          <h1>
            {system.name}
            <CopyButton text={system.name} className="copy" />
            <button className="copy" onClick={bookmarkHere} title="Save this system (and the body you're near) as a bookmark">
              Bookmark
            </button>
          </h1>
          <div className="muted">{bodiesSummary(system)}</div>
          <UnsoldSummary unsold={snapshot.unsold} />
        </div>
        <span className={`verdict verdict--${v.tone}`}>{v.label}</span>
      </section>

      <BookmarkBanner bookmarks={bookmarksFor(bookmarks, system.name)} openEditor={openEditor} />

      <section>
        <div className="section-head">
          <h2>Worth it</h2>
          <div className="sliders">
            <WorthSlider
              label="Min. mapped value"
              title="Planets worth less than this when mapped aren't listed (here or in the overlay)"
              steps={MAPPED_STEPS}
              command="set_worth_mapping_min"
              value={snapshot.worthMappingMin}
            />
            <WorthSlider
              label="Min. bio value"
              title="Planets whose best-case exobiology payout is below this aren't listed (here or in the overlay)"
              steps={BIO_STEPS}
              command="set_worth_bio_min"
              value={snapshot.worthBioMin}
            />
          </div>
        </div>
        {items.length === 0 ? (
          <p className="muted">Nothing with first credit worth stopping for.</p>
        ) : (
          <ul className="highlights">
            {items.map((i) => (
              <li key={i.key} className={i.done ? "done" : undefined}>
                <span className={`tag tag--${i.kind}`}>{i.kind === "map" ? "MAP" : "BIO"}</span>
                <strong>{i.body.shortName}</strong>
                <span>{i.kind === "map" ? bodyLabel(i.body) : bioDetail(i.body)}</span>
                <span className="muted">{i.done ? "done" : i.progress}</span>
                <span className="num">{formatRange(i.valueMin, i.value)}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <Exobiology
        bodies={system.bodies.filter(hasFirstBio)}
        hidden={system.bodies.filter((b) => b.bio && !hasFirstBio(b)).length}
      />

      <section>
        <div className="section-head">
          <h2>Bodies</h2>
          <label>
            <input type="checkbox" checked={onlyOpen} onChange={(e) => setOnlyOpen(e.target.checked)} />
            Only bodies with a first still open
          </label>
        </div>
        <table className="bodies">
          <thead>
            <tr>
              <th>Body</th>
              <th>Type</th>
              <th className="num">Distance</th>
              <th>Firsts open</th>
              <th>Signals</th>
              <th className="num">Scan</th>
              <th className="num">Mapped</th>
              <th>You</th>
            </tr>
          </thead>
          <tbody>
            {bodies.map((b) => (
              <BodyRow key={b.id} body={b} />
            ))}
          </tbody>
        </table>
        {bodies.length === 0 && <p className="muted">Every body here has already been claimed.</p>}
      </section>
    </div>
  );
}

function BodyRow({ body: b }: { body: Body }) {
  const firsts = openFirsts(b);
  const rings = b.rings.filter((r) => r.hotspots && r.hotspots.length > 0);
  const you = [
    b.mapped && (b.mappedEfficiently ? "mapped (efficient)" : "mapped"),
    b.footfall && "footfall",
    b.bioSignals > 0 && `bio ${speciesDone(b)}/${b.bioSignals}`,
  ].filter(Boolean);

  return (
    <>
      <tr>
        <td>
          <strong>{b.shortName}</strong>
        </td>
        <td>
          {bodyLabel(b)}
          {b.landable && <span className="chip chip--after">landable</span>}
        </td>
        <td className="num">{Math.round(b.distanceLs).toLocaleString()} ls</td>
        <td>
          {firsts.discovery && <span className="chip chip--first">Discovery</span>}
          {firsts.mapping && <span className="chip chip--first">Map</span>}
          {firsts.footfall && <span className="chip chip--first">Footfall</span>}
        </td>
        <td>
          {b.bioSignals > 0 && <span className="chip chip--bio">Bio {b.bioSignals}</span>}
          {b.geoSignals > 0 && <span className="chip">Geo {b.geoSignals}</span>}
        </td>
        <td className="num">{formatCredits(b.valueScan)}</td>
        <td className="num">{b.kind === "planet" ? formatCredits(b.valueMapped) : ""}</td>
        <td className="muted">{you.join(" · ")}</td>
      </tr>
      {rings.map((r) => (
        <tr key={r.name} className="ring-row">
          <td />
          <td colSpan={7}>
            <span className="muted">{r.shortName}:</span>{" "}
            {r.hotspots!.map((h) => `${h.kind} ×${h.count}`).join(", ")}
          </td>
        </tr>
      ))}
    </>
  );
}

/** Carried data that dying would lose. */
function UnsoldSummary({ unsold: u }: { unsold: Unsold }) {
  if (u.bioValue === 0 && u.cartoValue === 0) return null;
  const parts: string[] = [];
  if (u.bioValue > 0) parts.push(`bio ${formatCredits(u.bioValue)} (${u.bioSpecies} species)`);
  if (u.cartoValue > 0) {
    parts.push(`cartographic ≈${formatCredits(u.cartoValue)} (${u.cartoSystems} systems, ${u.cartoBodies} bodies)`);
  }
  return (
    <div
      className="muted"
      title="Estimated from the journals since your last death; cartographic values are rough and have run 20-60% high"
    >
      Unsold: {parts.join(" · ")}
    </div>
  );
}
