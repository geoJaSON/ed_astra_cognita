import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Bookmark, ImportResult, Snapshot } from "../../types";
import CopyButton from "../CopyButton";
import { distance, formatJumps, formatLy } from "../nearby/geo";
import { POS_SOURCE_LABELS, sameName, usedCategories, type OpenEditor } from "./useBookmarks";

type SortKey = "distance" | "name" | "added" | "updated";

const SORTS: { id: SortKey; label: string }[] = [
  { id: "distance", label: "Distance" },
  { id: "name", label: "Name" },
  { id: "added", label: "Recently added" },
  { id: "updated", label: "Recently updated" },
];

const JSON_FILTER = [{ name: "Bookmarks (JSON)", extensions: ["json"] }];

/** Today's local date as YYYY-MM-DD; toISOString would give the UTC date, a day off in the evening. */
function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Backend messages start lower case so they can be embedded; this makes one stand alone. */
const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

interface Props {
  snapshot: Snapshot;
  bookmarks: Bookmark[];
  openEditor: OpenEditor;
  /** Re-fetches the list; used after an import. */
  refresh(): void;
}

interface Row {
  bookmark: Bookmark;
  /** Distance from the current system; null when either position is unknown. */
  ly: number | null;
  isHere: boolean;
}

const byName = (a: Bookmark, b: Bookmark) =>
  a.system.localeCompare(b.system, undefined, { sensitivity: "base", numeric: true }) ||
  (a.body ?? "").localeCompare(b.body ?? "", undefined, { sensitivity: "base", numeric: true });

function compare(sort: SortKey, a: Row, b: Row): number {
  switch (sort) {
    case "distance":
      if (a.ly == null || b.ly == null) return (a.ly == null ? 1 : 0) - (b.ly == null ? 1 : 0) || byName(a.bookmark, b.bookmark);
      return a.ly - b.ly || byName(a.bookmark, b.bookmark);
    case "name":
      return byName(a.bookmark, b.bookmark);
    case "added":
      return b.bookmark.createdAt - a.bookmark.createdAt;
    case "updated":
      return b.bookmark.updatedAt - a.bookmark.updatedAt;
  }
}

export default function Bookmarks({ snapshot, bookmarks, openEditor, refresh }: Props) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<string[]>([]);
  const [sort, setSort] = useState<SortKey>("distance");
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

  const { system, jumpRange, bookmarksError, bookmarksLookupError } = snapshot;
  // Snapshots arrive often; only recompute distances when the system actually changes.
  const hereAddress = system?.address;
  const here = useMemo(() => (system ? { name: system.name, pos: system.starPos } : null), [hereAddress]);

  const categories = useMemo(() => usedCategories(bookmarks), [bookmarks]);
  const counts = useMemo(() => {
    const m = new Map<string, number>();
    for (const b of bookmarks) for (const c of new Set(b.categories.map((c) => c.toLowerCase()))) m.set(c, (m.get(c) ?? 0) + 1);
    return m;
  }, [bookmarks]);
  // Categories that disappeared (last bookmark using one deleted) stop filtering.
  const active = useMemo(() => filter.filter((f) => categories.some((c) => sameName(c, f))), [filter, categories]);

  const rows: Row[] = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return bookmarks
      .filter((b) => active.length === 0 || b.categories.some((c) => active.some((f) => sameName(c, f))))
      .filter((b) => {
        if (words.length === 0) return true;
        const text = [b.system, b.body ?? "", b.notes, ...b.categories].join("\n").toLowerCase();
        return words.every((w) => text.includes(w));
      })
      .map((b) => ({
        bookmark: b,
        ly: here && b.pos ? distance(b.pos, here.pos) : null,
        isHere: !!here && sameName(b.system, here.name),
      }))
      .sort((a, b) => compare(sort, a, b));
  }, [bookmarks, query, active, sort, here]);

  const toggleFilter = (c: string) =>
    setFilter((prev) => (prev.some((f) => sameName(f, c)) ? prev.filter((f) => !sameName(f, c)) : [...prev, c]));

  const exportAll = async () => {
    setStatus(null);
    try {
      const path = await save({
        title: "Export bookmarks",
        defaultPath: `ed-bookmarks-${today()}.json`,
        filters: JSON_FILTER,
      });
      if (!path) return;
      setBusy(true);
      const n = await invoke<number>("export_bookmarks", { path });
      setStatus({ text: `Exported ${n.toLocaleString()} bookmark${n === 1 ? "" : "s"}.`, error: false });
    } catch (e) {
      setStatus({ text: `Export failed: ${e}`, error: true });
    } finally {
      setBusy(false);
    }
  };

  const importFile = async () => {
    setStatus(null);
    try {
      const path = await open({ title: "Import bookmarks", multiple: false, directory: false, filters: JSON_FILTER });
      if (!path) return;
      setBusy(true);
      const r = await invoke<ImportResult>("import_bookmarks", { path });
      refresh();
      setStatus({ text: `Imported: ${r.added} added, ${r.updated} updated, ${r.skipped} skipped.`, error: false });
    } catch (e) {
      setStatus({ text: `Import failed: ${e}`, error: true });
    } finally {
      setBusy(false);
    }
  };

  const filtered = query.trim() !== "" || active.length > 0;

  return (
    <div className="bookmarks">
      <div className="bm-toolbar">
        <input
          type="search"
          className="bm-search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search systems, bodies, notes, categories"
          spellCheck={false}
        />
        <label className="bm-sort">
          Sort
          <select value={sort} onChange={(e) => setSort(e.target.value as SortKey)}>
            {SORTS.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
        <span className="bm-toolbar__spacer" />
        <button className="bm-primary" onClick={() => openEditor({ prefill: {} })}>
          Add bookmark
        </button>
        <button disabled={busy} onClick={importFile}>
          Import…
        </button>
        <button disabled={busy || bookmarks.length === 0} onClick={exportAll}>
          Export…
        </button>
      </div>

      {categories.length > 0 && (
        <div className="bm-filters">
          {categories.map((c) => {
            const on = active.some((f) => sameName(f, c));
            return (
              <button key={c} className={on ? "bm-filter bm-filter--on" : "bm-filter"} aria-pressed={on} onClick={() => toggleFilter(c)}>
                {c}
                <span className="bm-filter__count">{counts.get(c.toLowerCase()) ?? 0}</span>
              </button>
            );
          })}
          {active.length > 0 && (
            <button className="bm-link" onClick={() => setFilter([])}>
              Clear
            </button>
          )}
        </div>
      )}

      {bookmarksError && <div className="bm-alert">{capitalize(bookmarksError)}</div>}
      {status && <div className={status.error ? "bm-alert" : "bm-status"}>{status.text}</div>}

      {bookmarks.length === 0 ? (
        <div className="bm-empty">
          <p>
            <strong>No bookmarks yet.</strong>
          </p>
          <p className="muted">
            Bookmarks live in this app, not in the game. Elite doesn't let other apps read or change its own bookmarks, so these
            won't appear in the galaxy map there; use Copy to paste a system name into its search instead.
          </p>
          <p className="muted">Add one here, or use the Bookmark button on the Current System tab or on a POI in Nearby.</p>
        </div>
      ) : (
        <>
          <div className="bm-count muted">
            {filtered ? `${rows.length} of ${bookmarks.length}` : bookmarks.length} bookmark{bookmarks.length === 1 ? "" : "s"}
            {!here && " · distances show once your position is known"}
            <span className="bm-count__note"> · kept in this app, not in the game</span>
          </div>
          {rows.length === 0 ? (
            <p className="muted">
              No bookmarks match.{" "}
              <button
                className="bm-link"
                onClick={() => {
                  setQuery("");
                  setFilter([]);
                }}
              >
                Clear search and filters
              </button>
            </p>
          ) : (
            <ul className="bm-list">
              {rows.map((r) => (
                <BookmarkRow
                  key={r.bookmark.id}
                  row={r}
                  jumpRange={jumpRange}
                  lookupError={bookmarksLookupError}
                  onEdit={() => openEditor({ bookmark: r.bookmark })}
                />
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}

interface RowProps {
  row: Row;
  jumpRange: number | null;
  /** Snapshot.bookmarksLookupError */
  lookupError: string | null;
  onEdit(): void;
}

function BookmarkRow({ row, jumpRange, lookupError, onEdit }: RowProps) {
  const { bookmark: b } = row;
  return (
    <li className="bm-row">
      <CopyButton text={b.system} className="bm-copy" title={`Copy "${b.system}" to paste into the galaxy map`} />
      <div className="bm-row__main">
        <div className="bm-row__title">
          <strong className="bm-row__system">{b.system}</strong>
          {b.body && <span className="bm-row__body">{b.body}</span>}
          {b.categories.map((c) => (
            <span key={c} className="chip bm-chip">
              {c}
            </span>
          ))}
        </div>
        {b.notes && <Notes text={b.notes} />}
      </div>
      <div className="bm-row__dist">
        <Distance row={row} jumpRange={jumpRange} lookupError={lookupError} />
      </div>
      <button className="bm-row__edit" onClick={onEdit}>
        Edit
      </button>
    </li>
  );
}

function Distance({ row, jumpRange, lookupError }: Omit<RowProps, "onEdit">) {
  const { bookmark: b, ly } = row;
  if (row.isHere) return <span className="bm-here">You're here</span>;
  if (ly != null) {
    return (
      <span title={b.posSource ? `Position from ${POS_SOURCE_LABELS[b.posSource]}` : undefined}>
        <span className="num">{formatLy(ly)}</span>
        <span className="bm-row__jumps muted">{formatJumps(ly, jumpRange)}</span>
      </span>
    );
  }
  if (b.lookupFailed) {
    return (
      <span className="muted" title="EDSM has no record of this system; it may be undiscovered or misspelled">
        unknown to EDSM
      </span>
    );
  }
  if (!b.pos && lookupError) {
    return (
      <span className="muted" title={`Couldn't reach EDSM to look up the position; the app keeps trying. Last error: ${lookupError}`}>
        EDSM unreachable
      </span>
    );
  }
  if (!b.pos) return <span className="muted">looking up…</span>;
  return null;
}

/** Notes clamp to two lines, with a toggle when they're longer. */
function Notes({ text }: { text: string }) {
  const ref = useRef<HTMLParagraphElement>(null);
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);

  useLayoutEffect(() => {
    const el = ref.current!;
    if (expanded) return;
    const check = () => setOverflows(el.scrollHeight > el.clientHeight + 1);
    check();
    const observer = new ResizeObserver(check);
    observer.observe(el);
    return () => observer.disconnect();
  }, [text, expanded]);

  return (
    <div className="bm-notes">
      <p ref={ref} className={expanded ? "bm-notes__text" : "bm-notes__text bm-notes__text--clamped"}>
        {text}
      </p>
      {(overflows || expanded) && (
        <button className="bm-link" onClick={() => setExpanded(!expanded)}>
          {expanded ? "Less" : "More"}
        </button>
      )}
    </div>
  );
}
