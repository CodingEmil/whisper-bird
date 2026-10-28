import { useEffect, useState } from "react";
import { ChevronDown, Search, Sparkles, Trash2 } from "lucide-react";
import { api, formatDuration, type HistoryEntry } from "../lib/api";
import { Card, CopyButton, Row, Segmented, Select, useApp, useEvent } from "../lib/app";
import { useConfirm } from "../lib/confirm";
import { languageName, type TKey } from "../lib/i18n";

const SOURCE_KEY: Record<HistoryEntry["source"], TKey> = {
  dictation: "hist.dictation",
  enhanced: "hist.enhanced",
  edit: "hist.edit",
  file: "hist.file",
  meeting: "hist.meeting",
};

export function HistoryItem({ entry, compact, onDelete }: { entry: HistoryEntry; compact?: boolean; onDelete?: () => void }) {
  const { t, lang } = useApp();
  const [open, setOpen] = useState(false);
  const [flash, setFlash] = useState(false);
  const date = new Date(entry.created_at);
  const long = entry.text.length > 280;
  const hasRaw = entry.raw_text && entry.raw_text !== entry.text;

  return (
    <div
      className={`row top entry ${flash ? "flash" : ""}`}
      onDoubleClick={async () => {
        await api.copyText(entry.text);
        setFlash(true);
        setTimeout(() => setFlash(false), 500);
      }}
      title={t("hist.double_click")}
    >
      <div className="row-text">
        <div className="meta">
          <time dateTime={date.toISOString()}>{date.toLocaleString(lang, { dateStyle: compact ? undefined : "medium", timeStyle: "short" })}</time>
          {entry.app && <span className="app">{entry.app}</span>}
          {entry.language && <span>{languageName(entry.language)}</span>}
          {entry.duration_ms > 0 && <span className="num">{formatDuration(entry.duration_ms)}</span>}
          <span className={`badge ${entry.source === "enhanced" ? "accent" : ""}`}>
            {entry.source === "enhanced" && <Sparkles size={11} />}
            {t(SOURCE_KEY[entry.source])}
          </span>
        </div>
        <div className="entry-text selectable">
          {long && !open ? entry.text.slice(0, 280) + " …" : entry.text}
        </div>
        {open && hasRaw && (
          <div className="entry-raw selectable">
            <strong>{t("hist.raw")}:</strong> {entry.raw_text}
          </div>
        )}
      </div>
      <div className="row-control">
        {(long || hasRaw) && (
          <button type="button" className="btn ghost icon" aria-expanded={open} aria-label={t("hist.raw")} onClick={() => setOpen(!open)}>
            <ChevronDown size={16} style={{ transform: open ? "rotate(180deg)" : undefined }} />
          </button>
        )}
        <CopyButton text={entry.text} small />
        {onDelete && (
          <button type="button" className="btn ghost icon danger" aria-label={t("common.delete")} onClick={onDelete}>
            <Trash2 size={15} />
          </button>
        )}
      </div>
    </div>
  );
}

function toInputDate(ms: number | null) {
  if (!ms) return "";
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export default function History() {
  const { t, settings: s, update } = useApp();
  const confirm = useConfirm();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [search, setSearch] = useState("");
  const [source, setSource] = useState("all");
  const [from, setFrom] = useState<number | null>(null);
  const [to, setTo] = useState<number | null>(null);
  const [oldestFirst, setOldestFirst] = useState(false);

  const load = () =>
    api
      .historyQuery({ search, source: source === "all" ? null : source, from, to, oldest_first: oldestFirst, limit: 500 })
      .then(setEntries);

  useEffect(() => {
    const id = setTimeout(load, 150);
    return () => clearTimeout(id);
  }, [search, source, from, to, oldestFirst]);
  useEvent("history-added", load, [search, source, from, to, oldestFirst]);

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("hist.title")}</h1>
      </header>
      <p className="page-sub">{t("hist.double_click")}</p>

      <div className="toolbar" role="search">
        <div className="hstack">
          <div className="search">
            <Search size={15} />
            <input
              className="input wide"
              type="search"
              aria-label={t("common.search")}
              placeholder={t("common.search")}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          <Select
            value={oldestFirst ? "old" : "new"}
            label={t("hist.newest")}
            options={[
              ["new", t("hist.newest")],
              ["old", t("hist.oldest")],
            ]}
            onChange={(v) => setOldestFirst(v === "old")}
          />
        </div>
        <div className="hstack wrap">
          <Segmented
            value={source}
            label={t("hist.all")}
            options={[
              ["all", t("hist.all")],
              ["dictation", t("hist.dictation")],
              ["enhanced", t("hist.enhanced")],
              ["edit", t("hist.edit")],
              ["file", t("hist.file")],
              ["meeting", t("hist.meeting")],
            ]}
            onChange={setSource}
          />
          <div className="spacer" />
          {/* Zeitraum als Einheit, damit er beim Umbrechen nicht zerfällt. */}
          <div className="range">
            <label htmlFor="hist-from">{t("hist.from")}</label>
            <input
              id="hist-from"
              className="input"
              type="date"
              value={toInputDate(from)}
              onChange={(e) => setFrom(e.target.value ? new Date(e.target.value + "T00:00").getTime() : null)}
            />
            <label htmlFor="hist-to">{t("hist.to")}</label>
            <input
              id="hist-to"
              className="input"
              type="date"
              value={toInputDate(to)}
              onChange={(e) => setTo(e.target.value ? new Date(e.target.value + "T23:59:59").getTime() : null)}
            />
          </div>
        </div>
      </div>

      <Card>
        {entries.length === 0 ? (
          <div className="empty">{t("hist.empty")}</div>
        ) : (
          entries.map((e) => (
            <HistoryItem
              key={e.id}
              entry={e}
              onDelete={async () => {
                await api.historyDelete(e.id);
                setEntries((all) => all.filter((x) => x.id !== e.id));
              }}
            />
          ))
        )}
      </Card>

      <Card>
        <Row label={t("hist.limit")}>
          <Select
            value={String(s.history_limit)}
            options={[
              ["100", "100"],
              ["500", "500"],
              ["1000", "1.000"],
              ["5000", "5.000"],
              ["0", "∞"],
            ]}
            onChange={(v) => update({ history_limit: Number(v) })}
          />
        </Row>
        <Row label={t("hist.clear")}>
          <button
            type="button"
            className="btn danger"
            onClick={async () => {
              const ok = await confirm({
                title: t("hist.clear_confirm"),
                body: t("hist.clear_body"),
                action: t("hist.clear"),
              });
              if (ok) {
                await api.historyClear();
                load();
              }
            }}
          >
            <Trash2 size={15} /> {t("hist.clear")}
          </button>
        </Row>
      </Card>
    </>
  );
}
