import type { Bookmark } from "../../types";
import type { OpenEditor } from "./useBookmarks";

/** Your bookmarks for the system you're in, shown on the Current System tab. */
export default function BookmarkBanner({ bookmarks, openEditor }: { bookmarks: Bookmark[]; openEditor: OpenEditor }) {
  if (bookmarks.length === 0) return null;
  return (
    <section className="bm-banner">
      <div className="bm-banner__head">
        <span className="bm-star" aria-hidden>
          ★
        </span>
        Bookmarked
      </div>
      <ul className="bm-banner__list">
        {bookmarks.map((b) => (
          <li key={b.id} className="bm-banner__item">
            {b.body ? <strong>{b.body}</strong> : <span className="muted">System</span>}
            <span>
              {b.categories.map((c) => (
                <span key={c} className="chip bm-chip">
                  {c}
                </span>
              ))}
            </span>
            <span className="bm-banner__notes" title={b.notes || undefined}>
              {b.notes}
            </span>
            <button onClick={() => openEditor({ bookmark: b })}>Edit</button>
          </li>
        ))}
      </ul>
    </section>
  );
}
