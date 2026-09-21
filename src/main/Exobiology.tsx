import { bodyLabel, formatCredits, formatRange } from "../highlights";
import type { BioGenus, Body } from "../types";

interface Props {
  /** Bodies where the first-logged bonus is still available. */
  bodies: Body[];
  /** Bodies with bio that someone has already walked on, so aren't listed. */
  hidden: number;
}

export default function Exobiology({ bodies, hidden }: Props) {
  if (bodies.length === 0 && hidden === 0) return null;
  const sorted = [...bodies].sort((a, b) => (b.bio?.valueMax ?? 0) - (a.bio?.valueMax ?? 0));

  return (
    <section>
      <h2>Exobiology</h2>
      {sorted.length === 0 && <p className="muted">No bio left with first-logged credit.</p>}
      {sorted.map((b) => (
        <BioBody key={b.id} body={b} />
      ))}
      {hidden > 0 && (
        <p className="muted bio-hidden">
          {hidden} {hidden === 1 ? "body" : "bodies"} with bio already walked on, or in a populated system (no
          first-logged bonus).
        </p>
      )}
    </section>
  );
}

function BioBody({ body: b }: { body: Body }) {
  const bio = b.bio!;
  const status = bio.generaKnown ? "genera from DSS" : "predicted · map with DSS to confirm";

  return (
    <div className="bio-body">
      <div className="bio-body__head">
        <strong>{b.shortName}</strong>
        <span>{bodyLabel(b)}</span>
        <span className="muted">
          {bio.signals} signal{bio.signals === 1 ? "" : "s"} · {status}
        </span>
        <span className="num bio-body__value">
          {formatRange(bio.valueMin, bio.valueMax)}
          {!bio.valueComplete && "+"}
        </span>
      </div>
      <ul className="bio-genera">
        {bio.genera.map((g) => (
          <GenusRow key={g.name} genus={g} />
        ))}
      </ul>
    </div>
  );
}

function GenusRow({ genus: g }: { genus: BioGenus }) {
  const samples = g.sampled?.samples ?? 0;
  return (
    <li className={g.confirmed ? "bio-genus" : "bio-genus bio-genus--predicted"}>
      <span className="bio-genus__mark" title={g.confirmed ? "Confirmed" : "Possible"}>
        {samples >= 3 ? "✓" : g.confirmed ? "●" : "?"}
      </span>
      <span className="bio-genus__name">
        {g.name}
        {g.colonyDistanceM != null && <span className="muted"> · {g.colonyDistanceM} m</span>}
      </span>
      <span className="bio-genus__species">
        {g.sampled ? (
          <>
            {g.sampled.species}{" "}
            <span className="muted">{samples >= 3 ? "analysed" : samples === 0 ? "progress lost" : `${samples}/3`}</span>
          </>
        ) : g.candidates.length === 0 ? (
          <span className="muted">no known species match</span>
        ) : (
          g.candidates
            .slice()
            .reverse()
            .map((c) => (
              <span key={c.id} className="bio-candidate" title={c.colors.join(", ")}>
                {c.name.replace(`${g.name} `, "")} <span className="muted">{formatCredits(c.value)}</span>
              </span>
            ))
        )}
      </span>
    </li>
  );
}
