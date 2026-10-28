import { useState } from "react";
import { Plus, Trash2, X } from "lucide-react";
import type { Snippet } from "../lib/api";
import { Card, useApp } from "../lib/app";

export function ChipInput({ values, onChange, placeholder }: { values: string[]; onChange: (v: string[]) => void; placeholder: string }) {
  const { t } = useApp();
  const [draft, setDraft] = useState("");
  const add = () => {
    const parts = draft
      .split(/[,;\n]/)
      .map((p) => p.trim())
      .filter((p) => p && !values.includes(p));
    if (parts.length) onChange([...values, ...parts]);
    setDraft("");
  };
  return (
    <div className="vstack" style={{ gap: "var(--space-sm)" }}>
      <div className="hstack">
        <input
          className="input wide"
          type="text"
          aria-label={placeholder}
          value={draft}
          placeholder={placeholder}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <button type="button" className="btn" aria-label={t("common.add")} onClick={add} disabled={!draft.trim()}>
          <Plus size={15} />
        </button>
      </div>
      {values.length > 0 && (
        <div className="chips">
          {values.map((v) => (
            <span className="chip" key={v}>
              {v}
              <button type="button" aria-label={`${t("common.remove")}: ${v}`} onClick={() => onChange(values.filter((x) => x !== v))}>
                <X size={13} />
              </button>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export default function Words() {
  const { t, settings: s, update } = useApp();

  const setSnippets = (snippets: Snippet[]) => update({ snippets });

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("words.title")}</h1>
      </header>

      <Card title={t("words.custom")} sub={t("words.custom_hint")}>
        <div className="card-body">
          <ChipInput
            values={s.custom_words}
            onChange={(custom_words) => update({ custom_words })}
            placeholder={t("words.placeholder")}
          />
        </div>
      </Card>

      <Card title={t("words.snippets")} sub={t("words.snippets_hint")}>
        {s.snippets.map((sn, i) => (
          <div className="row fields" key={`${s.snippets.length}-${i}`}>
            <label className="field">
              <span>{t("words.trigger")}</span>
              <input
                className="input"
                type="text"
                defaultValue={sn.trigger}
                onBlur={(e) => setSnippets(s.snippets.map((x, j) => (j === i ? { ...x, trigger: e.target.value } : x)))}
              />
            </label>
            <label className="field">
              <span>{t("words.text")}</span>
              <textarea
                className="textarea"
                rows={3}
                defaultValue={sn.text}
                onKeyDown={(e) => {
                  // Tab im Snippet-Text erlauben.
                  if (e.key === "Tab") {
                    e.preventDefault();
                    const el = e.currentTarget;
                    const { selectionStart: a, selectionEnd: b, value } = el;
                    el.value = value.slice(0, a) + "\t" + value.slice(b);
                    el.selectionStart = el.selectionEnd = a + 1;
                  }
                }}
                onBlur={(e) => setSnippets(s.snippets.map((x, j) => (j === i ? { ...x, text: e.target.value } : x)))}
              />
            </label>
            <button
              type="button"
              className="btn ghost icon danger"
              aria-label={t("common.delete")}
              onClick={() => setSnippets(s.snippets.filter((_, j) => j !== i))}
            >
              <Trash2 size={15} />
            </button>
          </div>
        ))}
        <div className="row">
          <button type="button" className="btn small" onClick={() => setSnippets([...s.snippets, { trigger: "", text: "" }])}>
            <Plus size={14} /> {t("words.add_snippet")}
          </button>
        </div>
      </Card>
    </>
  );
}
