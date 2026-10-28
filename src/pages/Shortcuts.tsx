import { useState } from "react";
import { Info } from "lucide-react";
import type { Settings, ShortcutError } from "../lib/api";
import { Card, Row, ShortcutInput, useApp } from "../lib/app";
import type { TKey } from "../lib/i18n";

type Field = "shortcut_dictate" | "shortcut_dictate_enhance" | "shortcut_edit" | "shortcut_meeting";

const FIELDS: { field: Field; label: TKey; hint: TKey; optional: boolean }[] = [
  { field: "shortcut_dictate", label: "sc.dictate", hint: "sc.dictate_hint", optional: false },
  { field: "shortcut_dictate_enhance", label: "sc.dictate_enhance", hint: "sc.dictate_enhance_hint", optional: true },
  { field: "shortcut_edit", label: "sc.edit", hint: "sc.edit_hint", optional: true },
  { field: "shortcut_meeting", label: "sc.meeting", hint: "sc.meeting_hint", optional: true },
];

export default function Shortcuts() {
  const { t, settings: s, update } = useApp();
  const [errors, setErrors] = useState<ShortcutError[]>([]);
  const [conflict, setConflict] = useState<Field | null>(null);

  const set = async (field: Field, value: string) => {
    const taken = FIELDS.some((f) => f.field !== field && value && s[f.field] === value);
    if (taken) {
      setConflict(field);
      return;
    }
    setConflict(null);
    const patch: Partial<Settings> = { [field]: value };
    // Zweiter Shortcut gesetzt → Enhance-Modus passend umstellen.
    if (field === "shortcut_dictate_enhance" && value && s.enhance_mode === "off") {
      patch.enhance_mode = "second_shortcut";
    }
    setErrors(await update(patch));
  };

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("sc.title")}</h1>
      </header>
      <p className="page-sub">{t("sc.subtitle")}</p>
      <Card>
        {FIELDS.map((f) => {
          const err = errors.find((e) => e.field === f.field);
          return (
            <Row
              key={f.field}
              label={t(f.label)}
              hint={
                conflict === f.field ? (
                  <span style={{ color: "var(--bd-danger)" }}>{t("sc.conflict")}</span>
                ) : err ? (
                  <span style={{ color: "var(--bd-danger)" }}>{err.message}</span>
                ) : (
                  t(f.hint)
                )
              }
            >
              <ShortcutInput value={s[f.field]} onChange={(v) => set(f.field, v)} allowClear={f.optional} />
            </Row>
          );
        })}
      </Card>
      <div className="notice">
        <Info size={18} />
        <div>{t("sc.cancel_hint")}</div>
      </div>
    </>
  );
}
