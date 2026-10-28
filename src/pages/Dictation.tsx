import { useEffect, useState } from "react";
import { Volume2 } from "lucide-react";
import { api, type Activation, type MicDevice, type OverlayPosition } from "../lib/api";
import { Card, Row, Segmented, Select, Toggle, useApp } from "../lib/app";
import { LANGUAGES } from "../lib/i18n";

export default function Dictation() {
  const { t, settings: s, update } = useApp();
  const [mics, setMics] = useState<MicDevice[]>([]);

  useEffect(() => {
    api.listMicrophones().then(setMics);
  }, []);

  const activationHint: Record<Activation, string> = {
    toggle: t("dict.toggle_hint"),
    push_to_talk: t("dict.ptt_hint"),
    hybrid: t("dict.hybrid_hint"),
  };

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("dict.title")}</h1>
      </header>

      <Card>
        <Row label={t("dict.language")} hint={t("dict.language_hint")}>
          <Select
            value={s.language}
            options={[["auto", t("dict.auto")], ...LANGUAGES.map(([c, n]) => [c, n] as [string, string])]}
            onChange={(language) => update({ language })}
          />
        </Row>
        <Row label={t("dict.activation")} hint={activationHint[s.activation]}>
          <Segmented<Activation>
            value={s.activation}
            options={[
              ["toggle", t("dict.toggle")],
              ["push_to_talk", t("dict.ptt")],
              ["hybrid", t("dict.hybrid")],
            ]}
            onChange={(activation) => update({ activation })}
          />
        </Row>
        <Row label={t("dict.microphone")}>
          <Select
            value={s.microphone ?? ""}
            style={{ maxWidth: 320 }}
            options={[["", t("dict.default_mic")], ...mics.map((m) => [m.id, m.name] as [string, string])]}
            onChange={(id) => update({ microphone: id || null })}
          />
        </Row>
        <Row label={t("dict.fillers")} hint={t("dict.fillers_hint")}>
          <Toggle value={s.remove_fillers} onChange={(remove_fillers) => update({ remove_fillers })} />
        </Row>
      </Card>

      <Card title={t("dict.feedback")}>
        <Row label={t("dict.sounds")}>
          <Toggle value={s.sounds} onChange={(sounds) => update({ sounds })} />
        </Row>
        {s.sounds && (
          <Row label={t("dict.volume")}>
            <input
              className="slider"
              type="range"
              aria-label={t("dict.volume")}
              min={0.05}
              max={1}
              step={0.05}
              value={s.sound_volume}
              onChange={(e) => update({ sound_volume: Number(e.target.value) })}
              onMouseUp={() => api.playTestSound()}
            />
            <button type="button" className="btn ghost icon" aria-label={t("common.test")} onClick={() => api.playTestSound()}>
              <Volume2 size={16} />
            </button>
          </Row>
        )}
        <Row label={t("dict.ducking")} hint={t("dict.ducking_hint")}>
          <Toggle value={s.audio_ducking} onChange={(audio_ducking) => update({ audio_ducking })} />
        </Row>
        {s.audio_ducking && (
          <Row label={t("dict.ducking_level")}>
            <input
              className="slider"
              type="range"
              aria-label={t("dict.ducking_level")}
              min={0}
              max={0.8}
              step={0.05}
              value={s.ducking_level}
              onChange={(e) => update({ ducking_level: Number(e.target.value) })}
            />
            <span className="muted small num" style={{ minWidth: "2.5rem", textAlign: "right" }}>
              {Math.round(s.ducking_level * 100)} %
            </span>
          </Row>
        )}
      </Card>

      <Card title={t("dict.overlay")}>
        <Row label={t("dict.floating")} hint={t("dict.floating_hint")}>
          <Toggle value={s.show_floating_icon} onChange={(show_floating_icon) => update({ show_floating_icon })} />
        </Row>
        <Row label={t("dict.overlay_pos")} hint={t("dict.overlay_pos_hint")}>
          <Segmented<OverlayPosition>
            value={s.overlay_position}
            options={[
              ["top_center", t("dict.top")],
              ["bottom_center", t("dict.bottom")],
            ]}
            onChange={(overlay_position) => update({ overlay_position })}
          />
        </Row>
      </Card>
    </>
  );
}
