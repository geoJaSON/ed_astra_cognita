import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSnapshot } from "../useSnapshot";
import { bioDetail, bodyLabel, formatRange, highlights, type Highlight } from "../highlights";
import "./overlay.css";

const MAX_ROWS = 8;

export default function Overlay() {
  const snapshot = useSnapshot();
  if (!snapshot?.overlay.visible) return null;
  const { system, overlay } = snapshot;
  const items = system ? highlights(system, { mapped: snapshot.worthMappingMin, bio: snapshot.worthBioMin }) : [];

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
            <HighlightRow key={i.key} item={i} />
          ))}
        </ul>
      )}
      {items.length > MAX_ROWS && <div className="ov__more">+{items.length - MAX_ROWS} more</div>}
    </div>
  );
}

function HighlightRow({ item }: { item: Highlight }) {
  const { body, kind, done } = item;
  const detail = kind === "map" ? bodyLabel(body) : bioDetail(body);
  const firstTag = kind === "bio" ? "1st logged" : body.wasDiscovered ? "1st map" : "1st disc + map";

  return (
    <li className={done ? "hl hl--done" : "hl"}>
      <span className={`hl__tag hl__tag--${kind}`}>{kind === "map" ? "MAP" : "BIO"}</span>
      <span className="hl__body">{body.shortName}</span>
      <span className="hl__detail">
        {detail}
        <span className="hl__first">{firstTag}</span>
      </span>
      <span className="hl__status">{done ? "✓" : item.progress}</span>
      <span className="hl__value">{formatRange(item.valueMin, item.value)}</span>
    </li>
  );
}
