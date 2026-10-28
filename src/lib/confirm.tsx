import { createContext, useCallback, useContext, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Trash2, type LucideIcon } from "lucide-react";
import { useApp } from "./app";

export interface ConfirmOptions {
  title: string;
  body?: ReactNode;
  /** Beschriftung der Aktion, z. B. „Löschen“. */
  action: string;
  icon?: LucideIcon;
}

type Ask = (options: ConfirmOptions) => Promise<boolean>;

const ConfirmCtx = createContext<Ask | null>(null);

/**
 * Bestätigt eine zerstörerische Aktion in einem eigenen Dialog.
 * Nicht `window.confirm`: Das Dialog-Plugin von Tauri ersetzt es durch eine async-Funktion,
 * deren Promise in `if (confirm(…))` immer wahr ist – gelöscht würde vor der Antwort.
 */
export function ConfirmProvider({ children }: { children: ReactNode }) {
  const { t } = useApp();
  const [request, setRequest] = useState<ConfirmOptions | null>(null);
  const pending = useRef<((ok: boolean) => void) | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const cancelButton = useRef<HTMLButtonElement>(null);
  const pressedOnBackdrop = useRef(false);
  const titleId = useId();
  const bodyId = useId();

  const ask = useCallback<Ask>((options) => {
    pending.current?.(false); // Eine neue Frage verdrängt eine offene.
    return new Promise<boolean>((resolve) => {
      pending.current = resolve;
      setRequest(options);
    });
  }, []);

  const close = (ok: boolean) => {
    pending.current?.(ok);
    pending.current = null;
    setRequest(null);
  };

  useEffect(() => {
    const d = dialog.current;
    if (!request || !d) return;
    if (!d.open) d.showModal();
    // Abbrechen trägt den Fokus: Enter löscht nicht aus Versehen.
    cancelButton.current?.focus();
  }, [request]);

  useEffect(() => () => pending.current?.(false), []);

  const Icon = request?.icon ?? Trash2;
  return (
    <ConfirmCtx.Provider value={ask}>
      {children}
      {request && (
        <dialog
          ref={dialog}
          className="confirm"
          aria-labelledby={titleId}
          aria-describedby={request.body ? bodyId : undefined}
          onCancel={(e) => {
            e.preventDefault();
            close(false);
          }}
          onClose={() => close(false)}
          // Ein Klick auf den Hintergrund trifft den Dialog selbst, nicht seinen Inhalt.
          onMouseDown={(e) => (pressedOnBackdrop.current = e.target === e.currentTarget)}
          onClick={(e) => {
            if (pressedOnBackdrop.current && e.target === e.currentTarget) close(false);
          }}
        >
          <div className="confirm-inner">
            <span className="confirm-icon" aria-hidden="true">
              <Icon size={20} />
            </span>
            <div className="confirm-text">
              <h2 className="confirm-title" id={titleId}>{request.title}</h2>
              {request.body && <p className="confirm-body" id={bodyId}>{request.body}</p>}
            </div>
            <div className="confirm-actions">
              <button type="button" className="btn primary danger" onClick={() => close(true)}>
                {request.action}
              </button>
              <button type="button" className="btn" ref={cancelButton} onClick={() => close(false)}>
                {t("common.cancel")}
              </button>
            </div>
          </div>
        </dialog>
      )}
    </ConfirmCtx.Provider>
  );
}

export function useConfirm(): Ask {
  const ctx = useContext(ConfirmCtx);
  if (!ctx) throw new Error("ConfirmProvider fehlt");
  return ctx;
}
