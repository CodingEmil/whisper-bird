import { useEffect, useState } from "react";
import { ArrowRight, Boxes, Sparkles } from "lucide-react";
import { api, formatDuration, type HistoryEntry, type ModelStatus, type Stats } from "../lib/api";
import { Card, Kbd, useApp, useEvent } from "../lib/app";
import type { PageId } from "../App";
import { HistoryItem } from "./History";

/** Ersetzt „{shortcut}“ im Text durch die Tasten-Anzeige. */
export function WithShortcut({ text, shortcut }: { text: string; shortcut: string }) {
  const [before, after] = text.split("{shortcut}");
  return (
    <>
      {before}
      <span style={{ color: "var(--bd-text)" }}>
        <Kbd shortcut={shortcut} />
      </span>
      {after}
    </>
  );
}

export default function Home({ navigate }: { navigate: (p: PageId) => void }) {
  const { t, settings, lang } = useApp();
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [recent, setRecent] = useState<HistoryEntry[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [loaded, setLoaded] = useState<string | null>(null);

  const refresh = () => {
    api.listModels().then(setModels);
    api.historyQuery({ limit: 5 }).then(setRecent);
    api.stats().then(setStats);
    api.appInfo().then((i) => setLoaded(i.asr_loaded));
  };
  useEffect(refresh, []);
  useEvent("history-added", refresh);

  const asr = models.find((m) => m.id === settings.asr_model);
  const anyAsr = models.some((m) => m.kind === "asr" && m.installed);
  const savedMin = stats ? Math.max(0, stats.words / 40 - stats.duration_ms / 60000) : 0;
  const enhanceLabel =
    settings.enhance_mode === "off"
      ? t("enh.off")
      : `${settings.enhance_mode === "always" ? t("enh.always") : t("enh.second")} · ${
          settings.enhance_provider === "local" ? t("enh.local") : settings.enhance_provider.toUpperCase()
        }`;

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("nav.home")}</h1>
      </header>

      {!anyAsr && (
        <div className="notice warning" style={{ marginBottom: 16 }}>
          <Boxes size={18} />
          <div className="spacer">{t("home.no_model")}</div>
          <button className="btn small primary" onClick={() => navigate("models")}>
            {t("home.get_model")}
          </button>
        </div>
      )}

      <Card title={t("home.how")}>
        <ol className="steps">
          <li>{t("home.step1")}</li>
          <li>
            <WithShortcut text={t("home.step2")} shortcut={settings.shortcut_dictate} />
          </li>
          <li>{t("home.step3")}</li>
        </ol>
      </Card>

      <div className="grid-2">
        <Card title={t("home.status")}>
          <div className="row">
            <div className="row-text">
              <div className="row-label">{t("home.model")}</div>
              <div className="row-hint">
                {asr?.installed ? asr.name : t("home.no_model")}
                {loaded && asr?.id === loaded && <span className="badge success" style={{ marginLeft: 8 }}>{t("common.active")}</span>}
              </div>
            </div>
            <button className="btn ghost icon" onClick={() => navigate("models")}>
              <ArrowRight size={16} />
            </button>
          </div>
          <div className="row">
            <div className="row-text">
              <div className="row-label">
                <Sparkles size={14} style={{ verticalAlign: -2, marginRight: 4 }} />
                {t("home.enhance")}
              </div>
              <div className="row-hint">{enhanceLabel}</div>
            </div>
            <button className="btn ghost icon" onClick={() => navigate("enhance")}>
              <ArrowRight size={16} />
            </button>
          </div>
        </Card>
        <Card title={t("nav.stats")}>
          <div className="row">
            <div className="row-text">
              <div className="stat-value">{(stats?.words ?? 0).toLocaleString(lang)}</div>
              <div className="row-hint">{t("home.words_total")}</div>
            </div>
            <div className="row-text" style={{ textAlign: "right" }}>
              <div className="stat-value">{formatDuration(savedMin * 60000)}</div>
              <div className="row-hint">{t("home.time_saved")}</div>
            </div>
          </div>
        </Card>
      </div>

      <Card title={t("home.try")} sub={<WithShortcut text={t("home.try_hint")} shortcut={settings.shortcut_dictate} />}>
        <div className="card-body">
          <textarea className="textarea" rows={4} placeholder={t("home.try_placeholder")} />
        </div>
      </Card>

      <Card
        title={t("home.recent")}
        actions={
          <button className="btn ghost small" onClick={() => navigate("history")}>
            {t("nav.history")} <ArrowRight size={14} />
          </button>
        }
      >
        {recent.length === 0 ? (
          <div className="empty">{t("home.no_recent")}</div>
        ) : (
          recent.map((e) => <HistoryItem key={e.id} entry={e} compact />)
        )}
      </Card>
    </>
  );
}
