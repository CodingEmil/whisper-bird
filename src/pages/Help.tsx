import { Fragment, useEffect, useState } from "react";
import { ChevronDown, FileText } from "lucide-react";
import { api, type AppInfo } from "../lib/api";
import { Card, Row, useApp } from "../lib/app";
import type { TKey } from "../lib/i18n";

const FAQ: [TKey, TKey][] = [
  ["help.q1", "help.a1"],
  ["help.q2", "help.a2"],
  ["help.q3", "help.a3"],
  ["help.q4", "help.a4"],
  ["help.q5", "help.a5"],
  ["help.q6", "help.a6"],
];

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
        <Row label={t("help.version")}>
          <span className="mono">{info?.version}</span>
        </Row>
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
