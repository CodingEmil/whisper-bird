import { Info, Plus, RotateCcw, Trash2 } from "lucide-react";
import { api, type EditAction } from "../lib/api";
import { Card, Row, useApp } from "../lib/app";
import { WithShortcut } from "./Home";

export default function EditText() {
  const { t, settings: s, update } = useApp();

  const setActions = (edit_actions: EditAction[]) => update({ edit_actions });
  const patch = (i: number, p: Partial<EditAction>) =>
    setActions(s.edit_actions.map((a, j) => (j === i ? { ...a, ...p } : a)));

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("edit.title")}</h1>
      </header>
      <p className="page-sub">
        <WithShortcut text={t("edit.subtitle")} shortcut={s.shortcut_edit} />
      </p>

      <div className="notice">
        <Info size={18} />
        <div>{t("edit.needs_enhance")}</div>
      </div>

      <Card>
        <Row label={t("edit.translate_target")}>
          <input
            className="input"
            type="text"
            aria-label={t("edit.translate_target")}
            defaultValue={s.translate_target}
            onBlur={(e) => update({ translate_target: e.target.value.trim() || s.translate_target })}
          />
        </Row>
      </Card>

      <Card
        title={t("edit.actions")}
        actions={
          <>
            <button type="button" className="btn ghost small" onClick={async () => setActions(await api.defaultEditActions())}>
              <RotateCcw size={14} /> {t("edit.reset")}
            </button>
            <button
              type="button"
              className="btn small"
              onClick={() =>
                setActions([...s.edit_actions, { id: `custom-${Date.now()}`, label: "", instruction: "" }])
              }
            >
              <Plus size={14} /> {t("edit.add")}
            </button>
          </>
        }
      >
        {s.edit_actions.map((a, i) => (
          <div className="row fields" key={a.id}>
            <label className="field">
              <span>{t("edit.label")}</span>
              <input
                className="input"
                type="text"
                defaultValue={a.label}
                onBlur={(e) => patch(i, { label: e.target.value })}
              />
            </label>
            <label className="field">
              <span>{t("edit.instruction")}</span>
              <textarea
                className="textarea"
                rows={2}
                defaultValue={a.instruction}
                onBlur={(e) => patch(i, { instruction: e.target.value })}
              />
            </label>
            <button
              type="button"
              className="btn ghost icon danger"
              aria-label={t("common.delete")}
              onClick={() => setActions(s.edit_actions.filter((_, j) => j !== i))}
            >
              <Trash2 size={15} />
            </button>
          </div>
        ))}
      </Card>
    </>
  );
}
