import { useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Bookmark, BookmarkInput, Stop } from "../../types";
import { POS_SOURCE_LABELS, PRESET_CATEGORIES, sameName, uniqueNames, usedCategories, type EditorInit } from "./useBookmarks";

/** Suggestions offered for the system name. */
const SUGGESTION_LIMIT = 400;

interface Props {
  init: EditorInit;
  bookmarks: Bookmark[];
  /** Snapshot.bookmarksLookupError */
  lookupError: string | null;
  onSave(input: BookmarkInput): Promise<unknown>;
  onDelete(id: string): Promise<unknown>;
  onClose(): void;
}

export default function BookmarkEditor({ init, bookmarks, lookupError, onSave, onDelete, onClose }: Props) {
  const existing = "bookmark" in init ? init.bookmark : null;
  const prefill = "prefill" in init ? init.prefill : null;
  const start: { system?: string; body?: string | null; categories?: string[]; notes?: string } = existing ?? prefill ?? {};

  const [system, setSystem] = useState(start.system ?? "");
  const [body, setBody] = useState(start.body ?? "");
  const [categories, setCategories] = useState<string[]>(start.categories ?? []);
  const [notes, setNotes] = useState(start.notes ?? "");
  const [custom, setCustom] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [visited, setVisited] = useState<string[]>([]);

  const dialog = useRef<HTMLDialogElement>(null);
  const form = useRef<HTMLFormElement>(null);
  const systemInput = useRef<HTMLInputElement>(null);
  const notesInput = useRef<HTMLTextAreaElement>(null);

  // A modal <dialog> sits above the map and keeps focus inside itself.
  useEffect(() => {
    const d = dialog.current!;
    if (!d.open) d.showModal();
    if (!start.system) systemInput.current?.focus();
    else {
      const n = notesInput.current!;
      n.focus();
      n.setSelectionRange(n.value.length, n.value.length);
    }
    return () => d.close();
  }, []);

  useEffect(() => {
    invoke<Stop[]>("get_history")
      .then((stops) => setVisited(stops.map((s) => s.name).reverse()))
      .catch(() => {});
  }, []);

  const suggestions = useMemo(
    () => uniqueNames([...bookmarks.map((b) => b.system), ...visited]).slice(0, SUGGESTION_LIMIT),
    [bookmarks, visited],
  );
  const options = useMemo(
    () => uniqueNames([...PRESET_CATEGORIES, ...usedCategories(bookmarks), ...categories]),
    [bookmarks, categories],
  );

  const has = (c: string) => categories.some((x) => sameName(x, c));
  const toggle = (c: string) => setCategories((prev) => (prev.some((x) => sameName(x, c)) ? prev.filter((x) => !sameName(x, c)) : [...prev, c]));
  // Reuse an existing spelling so "exobiology" doesn't become a second category.
  const spelling = (name: string) => options.find((o) => sameName(o, name)) ?? name.trim();
  const addCustom = () => {
    if (!custom.trim()) return;
    const match = spelling(custom);
    setCategories((prev) => (prev.some((x) => sameName(x, match)) ? prev : [...prev, match]));
    setCustom("");
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy) return;
    const name = system.trim();
    if (!name) {
      setError("Enter a system name.");
      systemInput.current?.focus();
      return;
    }
    // A prefilled position only belongs to the prefilled system.
    const keepPos = prefill?.pos && prefill.system && sameName(name, prefill.system);
    const input: BookmarkInput = {
      id: existing?.id,
      system: name,
      body: body.trim() || null,
      // Text left in the "add category" box counts too, rather than being silently dropped.
      categories: uniqueNames([...categories, spelling(custom)]),
      notes: notes.trim(),
      pos: keepPos ? prefill.pos : null,
      posSource: keepPos ? (prefill.posSource ?? null) : null,
    };
    setBusy(true);
    setError(null);
    try {
      await onSave(input);
      onClose();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!existing) return;
    setBusy(true);
    setError(null);
    try {
      await onDelete(existing.id);
      onClose();
    } catch (err) {
      setError(String(err));
      setBusy(false);
      setConfirmDelete(false);
    }
  };

  const onCustomKey = (e: KeyboardEvent<HTMLInputElement>) => {
    // Enter adds the typed category; with the box empty it saves like the other fields.
    if (e.key === "Enter" && custom.trim()) {
      e.preventDefault();
      addCustom();
    }
  };

  const onNotesKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      form.current?.requestSubmit();
    }
  };

  return (
    <dialog
      ref={dialog}
      className="bm-editor"
      aria-labelledby="bm-editor-title"
      onCancel={(e) => {
        // Escape backs out of the delete confirmation first.
        e.preventDefault();
        if (confirmDelete) setConfirmDelete(false);
        else onClose();
      }}
      onClose={() => {
        // The browser can close it without a cancel event (e.g. a repeated Escape).
        if (!dialog.current?.open) onClose();
      }}
    >
      <form ref={form} className="bm-editor__form" onSubmit={submit}>
        <div id="bm-editor-title" className="bm-editor__title">
          {existing ? "Edit bookmark" : "New bookmark"}
        </div>

        <div className="bm-editor__row">
          <label className="bm-field bm-field--grow">
            <span>System</span>
            <input
              ref={systemInput}
              value={system}
              onChange={(e) => setSystem(e.target.value)}
              list="bm-system-suggestions"
              placeholder="System name"
              spellCheck={false}
              autoComplete="off"
              aria-required
            />
          </label>
          <label className="bm-field">
            <span>Body</span>
            <input value={body} onChange={(e) => setBody(e.target.value)} placeholder="e.g. 3 b (optional)" spellCheck={false} />
          </label>
        </div>
        <datalist id="bm-system-suggestions">
          {suggestions.map((s) => (
            <option key={s} value={s} />
          ))}
        </datalist>
        {/* The live copy, so a lookup finishing while the editor is open shows up. */}
        <PositionNote
          existing={existing && (bookmarks.find((b) => b.id === existing.id) ?? existing)}
          prefill={prefill}
          system={system}
          lookupError={lookupError}
        />

        <div className="bm-field">
          <span>Categories</span>
          <div className="bm-cats">
            {options.map((c) => (
              <button
                key={c}
                type="button"
                className={has(c) ? "bm-cat bm-cat--on" : "bm-cat"}
                aria-pressed={has(c)}
                onClick={() => toggle(c)}
              >
                {c}
              </button>
            ))}
            <span className="bm-cats__add">
              <input value={custom} onChange={(e) => setCustom(e.target.value)} onKeyDown={onCustomKey} placeholder="New category" />
              <button type="button" disabled={!custom.trim()} onClick={addCustom}>
                Add
              </button>
            </span>
          </div>
        </div>

        <label className="bm-field">
          <span>Notes</span>
          <textarea ref={notesInput} value={notes} onChange={(e) => setNotes(e.target.value)} onKeyDown={onNotesKey} rows={4} placeholder="Ctrl+Enter saves" />
        </label>

        {error && <div className="bm-editor__error">{error}</div>}

        <div className="bm-editor__buttons">
          {existing &&
            (confirmDelete ? (
              <span className="bm-editor__confirm">
                Delete this bookmark?
                <button type="button" className="bm-danger" disabled={busy} onClick={remove}>
                  Delete
                </button>
                <button type="button" onClick={() => setConfirmDelete(false)}>
                  Keep
                </button>
              </span>
            ) : (
              <button type="button" className="bm-danger-quiet" disabled={busy} onClick={() => setConfirmDelete(true)}>
                Delete
              </button>
            ))}
          <span className="bm-editor__spacer" />
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="bm-primary" disabled={busy || !system.trim()}>
            {busy && !confirmDelete ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
    </dialog>
  );
}

/** Where the bookmark's position comes from, so a missing distance isn't a mystery. */
interface PositionNoteProps {
  existing: Bookmark | null;
  prefill: Partial<BookmarkInput> | null;
  system: string;
  lookupError: string | null;
}

function PositionNote({ existing, prefill, system, lookupError }: PositionNoteProps) {
  const name = system.trim();
  if (!name) return null;
  let text: string;
  if (existing) {
    if (!sameName(name, existing.system)) text = "The position will be looked up again for the new name.";
    else if (existing.pos && existing.posSource) text = `Position from ${POS_SOURCE_LABELS[existing.posSource]}.`;
    else if (existing.lookupFailed) text = "EDSM doesn't know this system, so there's no position or distance.";
    else if (lookupError) text = `Couldn't reach EDSM for the position; the app keeps trying (${lookupError}).`;
    else text = "Looking up the position…";
  } else if (prefill?.pos && prefill.posSource && prefill.system && sameName(name, prefill.system)) {
    text = `Position from ${POS_SOURCE_LABELS[prefill.posSource]}.`;
  } else {
    text = "The position comes from your journal if you've been there or it's on your route, otherwise from EDSM.";
  }
  return <div className="bm-editor__note muted">{text}</div>;
}
