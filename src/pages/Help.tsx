import { Fragment, useEffect, useState } from "react";
import { ChevronDown, Download, ExternalLink, FileText, RefreshCw } from "lucide-react";
import { api, formatBytes, type AppInfo } from "../lib/api";
import { Card, Progress, Row, useApp } from "../lib/app";
import type { TKey } from "../lib/i18n";
import { useUpdate } from "../lib/update";

const FAQ: [TKey, TKey][] = [
  ["help.q1", "help.a1"],
  ["help.q2", "help.a2"],
  ["help.q3", "help.a3"],
  ["help.q4", "help.a4"],
  ["help.q5", "help.a5"],
  ["help.q6", "help.a6"],
];

/** Version, Stand der Update-Suche und, wenn es eine neue Version gibt, deren Installation. */
function VersionRow() {
  const { t, lang } = useApp();
  const { status, progress } = useUpdate();
  const [error, setError] = useState("");
  if (!status) return null;

  // Das Update lädt schon im Hintergrund, sobald es gefunden ist.
  const loading = progress?.state === "downloading" || progress?.state === "verifying";
  const starting = status.installing && !loading;
  const failed = progress?.state === "error" ? progress.error : error;
  const text = status.checking
    ? t("update.checking")
    : status.latest
      ? t(status.installable ? "update.ready" : "update.ready_manual", { version: status.latest })
      : status.error
        ? t("update.failed_detail", { error: status.error })
        : status.checked_at
          ? t("update.checked", { time: new Date(status.checked_at).toLocaleString(lang, { dateStyle: "medium", timeStyle: "short" }) })
          : null;

  const install = async () => {
    setError("");
    try {
      await api.updateInstall();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Row
      label={t("help.version")}
      hint={
        <>
          {text}
          {loading && progress && (
            <div className="download">
              <Progress value={progress.total > 0 ? progress.downloaded / progress.total : 0} />
              <div className="small muted">
                {progress.state === "verifying"
                  ? t("common.verifying")
                  : `${formatBytes(progress.downloaded)} / ${formatBytes(progress.total)}`}
              </div>
            </div>
          )}
          {starting && <div>{t("update.starting")}</div>}
          {failed && <div className="error-text" role="alert">{failed}</div>}
        </>
      }
    >
      <span className="mono">{status.current}</span>
      {!status.latest ? (
        <button type="button" className="btn small" disabled={status.checking} onClick={() => void api.updateCheck()}>
          <RefreshCw size={14} className={status.checking ? "spin" : undefined} /> {t("update.check")}
        </button>
      ) : status.installable ? (
        <>
          <button type="button" className="btn ghost small" onClick={() => api.updateOpenPage()}>
            {t("update.notes")}
          </button>
          <button
            type="button"
            className="btn primary small"
            disabled={status.installing}
            aria-busy={status.installing}
            onClick={install}
          >
            <Download size={14} /> {t("update.install")}
          </button>
        </>
      ) : (
        <button type="button" className="btn primary small" onClick={() => api.updateOpenPage()}>
          <ExternalLink size={14} /> {t("update.download")}
        </button>
      )}
    </Row>
  );
}

export default function Help() {
  const { t } = useApp();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [open, setOpen] = useState<number | null>(0);

  useEffect(() => {
    api.appInfo().then(setInfo);
  }, []);

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("help.title")}</h1>
      </header>

      <Card title={t("help.faq")}>
        {FAQ.map(([q, a], i) => (
          <Fragment key={q}>
            <button
              type="button"
              className="row clickable"
              aria-expanded={open === i}
              aria-controls={`faq-${i}`}
              onClick={() => setOpen(open === i ? null : i)}
            >
              <span className="row-label spacer">{t(q)}</span>
              <ChevronDown size={16} className="chev" />
            </button>
            {open === i && <div className="answer" id={`faq-${i}`}>{t(a)}</div>}
          </Fragment>
        ))}
      </Card>

      <Card>
        <VersionRow />
        <Row label={t("help.devices")} hint={info?.devices.join(", ") || "CPU"} />
        <Row label={t("help.credits")} hint={t("help.credits_text")} />
        <Row label={t("help.notices")} hint={t("help.notices_hint")}>
          <button type="button" className="btn small" onClick={() => api.openNotices()}>
            <FileText size={14} /> {t("common.open")}
          </button>
        </Row>
      </Card>
    </>
  );
}
