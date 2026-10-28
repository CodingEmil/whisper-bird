import { useEffect, useMemo, useState } from "react";
import { ChartColumn, List, RotateCcw } from "lucide-react";
import { api, formatDuration, type Stats as StatsData } from "../lib/api";
import { Card, Segmented, useApp, useEvent } from "../lib/app";
import { useConfirm } from "../lib/confirm";
import { languageName } from "../lib/i18n";

function compact(n: number, lang: string) {
  return new Intl.NumberFormat(lang, { notation: n >= 10000 ? "compact" : "standard", maximumFractionDigits: 1 }).format(n);
}

/** Eine Zahl mit ihrer Beschriftung; Geschwister trennt eine Haarlinie. */
function Tile({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div>
      <div className="figure-label">{label}</div>
      <div className="figure-value">{value}</div>
      {hint && <div className="figure-hint">{hint}</div>}
    </div>
  );
}

/** Wörter pro Tag, letzte 30 Tage: eine Reihe, ein Farbton. */
function DailyChart({ days }: { days: StatsData["days"] }) {
  const { t, lang } = useApp();
  const [hover, setHover] = useState<number | null>(null);
  const [table, setTable] = useState(false);

  const series = useMemo(() => {
    const map = new Map(days.map((d) => [d.day, d]));
    const out: { day: string; words: number; count: number }[] = [];
    const now = new Date();
    for (let i = 29; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
      const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      const v = map.get(key);
      out.push({ day: key, words: v?.words ?? 0, count: v?.count ?? 0 });
    }
    return out;
  }, [days]);

  const max = Math.max(...series.map((s) => s.words), 1);
  const niceMax = (() => {
    const p = Math.pow(10, Math.floor(Math.log10(max)));
    return Math.ceil(max / p) * p;
  })();
  const fmtDay = (key: string, style: "short" | "long") =>
    new Date(key + "T12:00").toLocaleDateString(lang, style === "short" ? { day: "numeric", month: "numeric" } : { weekday: "short", day: "numeric", month: "long" });

  return (
    <Card
      title={t("stats.last30")}
      sub={t("stats.words")}
      actions={
        <Segmented
          value={table ? "table" : "chart"}
          label={t("stats.last30")}
          options={[
            ["chart", <><ChartColumn size={15} /><span className="visually-hidden">{t("stats.chart")}</span></>],
            ["table", <><List size={15} /><span className="visually-hidden">{t("stats.table")}</span></>],
          ]}
          onChange={(v) => setTable(v === "table")}
        />
      }
    >
      {table ? (
        <div className="card-body" style={{ maxHeight: "16rem", overflowY: "auto" }}>
          <table className="table">
            <tbody>
              {[...series].reverse().map((s) => (
                <tr key={s.day}>
                  <td>{fmtDay(s.day, "long")}</td>
                  <td>{s.words.toLocaleString(lang)}</td>
                  <td className="muted">
                    {s.count} × {t("stats.transcriptions")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="card-body">
          <div className="daily-chart">
            <div className="daily-axis">
              <span>{compact(niceMax, lang)}</span>
              <span>{compact(niceMax / 2, lang)}</span>
              <span>0</span>
            </div>
            <div className="daily-plot" onMouseLeave={() => setHover(null)}>
              <div className="daily-grid" style={{ top: 0 }} />
              <div className="daily-grid" style={{ top: "50%" }} />
              {series.map((s, i) => (
                <div key={s.day} className="daily-slot" onMouseEnter={() => setHover(i)}>
                  <div
                    className={`daily-bar ${hover === i ? "hover" : ""}`}
                    style={{ height: `${(s.words / niceMax) * 100}%`, minHeight: s.words > 0 ? 2 : 0 }}
                  />
                </div>
              ))}
              {hover !== null && (
                <div
                  className="daily-tip"
                  style={{ left: `${((hover + 0.5) / series.length) * 100}%` }}
                >
                  <div className="small muted">{fmtDay(series[hover].day, "long")}</div>
                  <div className="num" style={{ fontWeight: "var(--weight-strong)" }}>
                    {series[hover].words.toLocaleString(lang)} {t("stats.words")}
                  </div>
                  <div className="small muted">
                    {series[hover].count} {t("stats.transcriptions")}
                  </div>
                </div>
              )}
            </div>
          </div>
          <div className="daily-labels">
            <span>{fmtDay(series[0].day, "short")}</span>
            <span>{fmtDay(series[14].day, "short")}</span>
            <span>{fmtDay(series[29].day, "short")}</span>
          </div>
        </div>
      )}
    </Card>
  );
}

export default function Stats() {
  const { t, lang } = useApp();
  const confirm = useConfirm();
  const [stats, setStats] = useState<StatsData | null>(null);
  const load = () => api.stats().then(setStats);
  useEffect(() => {
    load();
  }, []);
  useEvent("history-added", load);

  if (!stats) return null;
  const minutes = stats.duration_ms / 60000;
  const wpm = minutes > 0.1 ? Math.round(stats.words / minutes) : 0;
  const savedMs = Math.max(0, (stats.words / 40) * 60000 - stats.duration_ms);
  const maxLang = Math.max(...stats.languages.map((l) => l.count), 1);

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("stats.title")}</h1>
      </header>
      {stats.first_use && (
        <p className="page-sub">{t("stats.since", { date: new Date(stats.first_use).toLocaleDateString(lang, { dateStyle: "long" }) })}</p>
      )}

      <div className="figures">
        <Tile label={t("stats.transcriptions")} value={compact(stats.count, lang)} />
        <Tile label={t("stats.words")} value={compact(stats.words, lang)} />
        <Tile label={t("stats.duration")} value={formatDuration(stats.duration_ms)} />
        <Tile label={t("stats.saved")} value={formatDuration(savedMs)} hint={t("stats.saved_hint")} />
      </div>
      <div className="figures minor five">
        <Tile label={t("stats.wpm")} value={String(wpm)} />
        <Tile label={t("stats.enhanced")} value={compact(stats.enhanced, lang)} />
        <Tile label={t("stats.edits")} value={compact(stats.edits, lang)} />
        <Tile label={t("stats.files")} value={compact(stats.files, lang)} />
        <Tile label={t("stats.meetings")} value={compact(stats.meetings, lang)} />
      </div>

      <DailyChart days={stats.days} />

      <Card title={t("stats.languages")}>
        <div className="card-body lang-list">
          {stats.languages.length === 0 && <div className="muted">{t("common.none")}</div>}
          {stats.languages.map((l) => (
            <div key={l.language} className="lang-row" title={`${l.words.toLocaleString(lang)} ${t("stats.words")}`}>
              <span className="lang-name">{languageName(l.language)}</span>
              <div className="lang-track">
                <div className="lang-bar" style={{ width: `${(l.count / maxLang) * 100}%` }} />
              </div>
              <span className="lang-value">{l.count.toLocaleString(lang)}</span>
            </div>
          ))}
        </div>
      </Card>

      <div className="page-foot">
        <button
          type="button"
          className="btn danger"
          onClick={async () => {
            const ok = await confirm({
              title: t("stats.reset_confirm"),
              body: t("stats.reset_body"),
              action: t("common.reset"),
              icon: RotateCcw,
            });
            if (ok) {
              await api.resetStats();
              load();
            }
          }}
        >
          {t("stats.reset")}
        </button>
      </div>
    </>
  );
}
