import { useEffect, useMemo, useRef, useState } from "react";

import { useT } from "../hooks/useT";
import { useImagoStore } from "../store/useImagoStore";
import {
  type FileEntry,
  type SortKey,
  type StatusFilter,
  statusCounts,
  visibleEntries,
} from "../utils/files";
import { FileCard } from "./FileCard";
import { IconSort } from "./icons";
import { SelectBox } from "./ui/controls";

/** Fixed row height (px) — must match `.file-card` height in CSS. */
export const ROW_HEIGHT = 84;
const OVERSCAN = 6;

const FILTERS: StatusFilter[] = ["all", "pending", "processing", "done", "skipped", "cancelled", "failed"];
const SORTS: SortKey[] = ["added", "name", "size", "saved", "status"];

/** Which rows of a virtual list are visible. Exported for tests. */
export function visibleRange(scrollTop: number, viewport: number, count: number): { start: number; end: number } {
  const start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const end = Math.min(count, Math.ceil((scrollTop + viewport) / ROW_HEIGHT) + OVERSCAN);
  return { start, end };
}

function FileToolbar({ counts }: { counts: Record<StatusFilter, number> }) {
  const t = useT();
  const filter = useImagoStore((state) => state.filter);
  const sort = useImagoStore((state) => state.sort);
  const direction = useImagoStore((state) => state.direction);
  const setFilter = useImagoStore((state) => state.setFilter);
  const setSort = useImagoStore((state) => state.setSort);
  const toggleDirection = useImagoStore((state) => state.toggleDirection);
  const clearAll = useImagoStore((state) => state.clearAll);
  const clearFinished = useImagoStore((state) => state.clearFinished);
  const running = useImagoStore((state) => state.job.running);

  return (
    <div className="file-toolbar">
      <div className="chips" role="group" aria-label={t("filter.label")}>
        {FILTERS.filter((value) => value === "all" || counts[value] > 0).map((value) => (
          <button
            key={value}
            type="button"
            className={`chip${filter === value ? " is-selected" : ""}`}
            aria-pressed={filter === value}
            onClick={() => setFilter(value)}
          >
            {value === "all" ? t("filter.all") : t(`status.${value}`)}
            <span className="chip__count">{counts[value]}</span>
          </button>
        ))}
      </div>
      <div className="file-toolbar__right">
        <label className="visually-hidden" htmlFor="sort-select">
          {t("sort.label")}
        </label>
        <SelectBox
          id="sort-select"
          small
          value={sort}
          onChange={setSort}
          options={SORTS.map((value) => ({ value, label: t(`sort.${value}`) }))}
        />
        <button
          type="button"
          className={`icon-button${direction === "desc" ? " is-flipped" : ""}`}
          onClick={toggleDirection}
          aria-label={direction === "asc" ? t("sort.ascending") : t("sort.descending")}
          title={direction === "asc" ? t("sort.ascending") : t("sort.descending")}
        >
          <IconSort />
        </button>
        {counts.done > 0 && (
          <button type="button" className="button button--ghost button--small" onClick={clearFinished} disabled={running}>
            {t("actions.clearDone")}
          </button>
        )}
        <button type="button" className="button button--ghost button--small" onClick={clearAll} disabled={running}>
          {t("actions.clear")}
        </button>
      </div>
    </div>
  );
}

/** Virtualized list: only the visible rows are rendered, so 10 000+ files stay smooth. */
export function FileList() {
  const t = useT();
  const entries = useImagoStore((state) => state.entries);
  const order = useImagoStore((state) => state.order);
  const filter = useImagoStore((state) => state.filter);
  const sort = useImagoStore((state) => state.sort);
  const direction = useImagoStore((state) => state.direction);
  const jobRunning = useImagoStore((state) => state.job.running);

  const all = useMemo(
    () => order.map((id) => entries[id]).filter((entry): entry is FileEntry => Boolean(entry)),
    [entries, order],
  );
  const counts = useMemo(() => statusCounts(all), [all]);
  const rows = useMemo(() => visibleEntries(all, filter, sort, direction), [all, filter, sort, direction]);

  const viewportRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(600);

  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return undefined;
    setViewport(element.clientHeight);
    if (typeof ResizeObserver === "undefined") return undefined;
    const observer = new ResizeObserver(() => setViewport(element.clientHeight));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const { start, end } = visibleRange(scrollTop, viewport, rows.length);

  return (
    <section className="file-section" aria-label={t("list.label")}>
      <FileToolbar counts={counts} />
      <div
        ref={viewportRef}
        className="file-list"
        role="list"
        aria-label={t("list.count", { n: rows.length })}
        onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      >
        {rows.length === 0 ? (
          <p className="file-list__empty">{t("list.empty")}</p>
        ) : (
          <div className="file-list__spacer" style={{ height: rows.length * ROW_HEIGHT }}>
            <div className="file-list__window" style={{ transform: `translateY(${start * ROW_HEIGHT}px)` }}>
              {rows.slice(start, end).map((entry) => (
                <FileCard key={entry.id} entry={entry} jobRunning={jobRunning} />
              ))}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
