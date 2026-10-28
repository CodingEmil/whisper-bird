import { FolderOpen } from "lucide-react";
import { api, type Accelerator, type PasteMethod } from "../lib/api";
import { Card, Row, Segmented, Select, Toggle, useApp } from "../lib/app";
import { ChipInput } from "./Words";

export default function Advanced() {
  const { t, settings: s, update } = useApp();

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("adv.title")}</h1>
      </header>

      <Card title={t("adv.insert")}>
        <Row label={t("adv.method")} hint={t("adv.method_hint")}>
          <Segmented<PasteMethod>
            value={s.paste_method}
            options={[
              ["clipboard", t("adv.clipboard")],
              ["type", t("adv.type")],
            ]}
            onChange={(paste_method) => update({ paste_method })}
          />
        </Row>
        {s.paste_method === "clipboard" && (
          <Row label={t("adv.preserve")} hint={t("adv.preserve_hint")}>
            <Toggle value={s.preserve_clipboard} onChange={(preserve_clipboard) => update({ preserve_clipboard })} />
          </Row>
        )}
        <Row label={t("adv.trailing_space")}>
          <Toggle value={s.trailing_space} onChange={(trailing_space) => update({ trailing_space })} />
        </Row>
        <Row label={t("adv.auto_enter")} hint={t("adv.auto_enter_hint")} stack>
          <ChipInput
            values={s.auto_enter_apps}
            onChange={(apps) =>
              update({
                auto_enter_apps: apps.map((a) => {
                  const x = a.toLowerCase();
                  return x.endsWith(".exe") ? x : `${x}.exe`;
                }),
              })
            }
            placeholder={t("adv.auto_enter_placeholder")}
          />
        </Row>
      </Card>

      <Card title={t("adv.system")}>
        <Row label={t("adv.ui_language")}>
          <Segmented
            value={s.ui_language}
            options={[
              ["de", "Deutsch"],
              ["en", "English"],
            ]}
            onChange={(ui_language) => update({ ui_language })}
          />
        </Row>
        <Row label={t("adv.autostart")}>
          <Toggle value={s.autostart} onChange={(autostart) => update({ autostart })} />
        </Row>
        <Row label={t("adv.start_hidden")}>
          <Toggle value={s.start_hidden} onChange={(start_hidden) => update({ start_hidden })} />
        </Row>
      </Card>

      <Card title={t("adv.performance")}>
        <Row label={t("adv.accelerator")}>
          <Select<Accelerator>
            value={s.accelerator}
            options={[
              ["auto", t("adv.accel_auto")],
              ["gpu", t("adv.accel_gpu")],
              ["cpu", t("adv.accel_cpu")],
            ]}
            onChange={(accelerator) => update({ accelerator })}
          />
        </Row>
        <Row label={t("adv.unload")}>
          <Select
            value={String(s.unload_after_min)}
            options={[
              ["0", t("adv.never")],
              ["5", t("adv.minutes", { n: 5 })],
              ["15", t("adv.minutes", { n: 15 })],
              ["60", t("adv.minutes", { n: 60 })],
            ]}
            onChange={(v) => update({ unload_after_min: Number(v) })}
          />
        </Row>
      </Card>

      <Card title={t("adv.debug")}>
        <Row label={t("adv.debug_log")} hint={t("adv.debug_log_hint")}>
          <Toggle value={s.debug_logging} onChange={(debug_logging) => update({ debug_logging })} />
        </Row>
        <Row label={t("adv.open_logs")}>
          <button type="button" className="btn small" onClick={() => api.openDataDir("logs")}>
            <FolderOpen size={14} /> {t("common.open")}
          </button>
        </Row>
        <Row label={t("adv.open_data")}>
          <button type="button" className="btn small" onClick={() => api.openDataDir("data")}>
            <FolderOpen size={14} /> {t("common.open")}
          </button>
        </Row>
      </Card>
    </>
  );
}
