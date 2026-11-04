// Freigaben unter macOS (Bedienungshilfen, Mikrofon): Status abfragen und anfordern.
// Unter Windows gibt es nichts freizugeben; dort bleibt alles still.

import { useEffect, useState } from "react";
import { Bird } from "./bird";
import { Check } from "lucide-react";
import { api, on, type PermissionKind, type Permissions } from "./api";
import { Card, Row, useApp } from "./app";
import { isMac } from "./platform";

/** Aktueller Stand; wird laufend nachgefragt, weil die Freigabe in den Systemeinstellungen passiert. */
export function usePermissions(): Permissions | null {
  const [perms, setPerms] = useState<Permissions | null>(null);
  useEffect(() => {
    if (!isMac) return;
    let alive = true;
    const load = () =>
      api
        .permissionsStatus()
        .then((p) => alive && setPerms(p))
        .catch(() => {});
    load();
    const timer = window.setInterval(load, 2000);
    const off = on("permissions-changed", load);
    return () => {
      alive = false;
      window.clearInterval(timer);
      off();
    };
  }, []);
  return perms;
}

function Action({
  kind,
  granted,
  denied,
  onRequest,
}: {
  kind: PermissionKind;
  granted: boolean;
  denied: boolean;
  onRequest?: () => void;
}) {
  const { t } = useApp();
  if (granted) {
    return (
      <span className="badge success">
        <Check size={11} /> {t("perm.granted")}
      </span>
    );
  }
  if (denied) {
    return (
      <button type="button" className="btn small" onClick={() => api.permissionOpenSettings(kind)}>
        {t("perm.open_settings")}
      </button>
    );
  }
  return (
    <button
      type="button"
      className="btn small primary"
      onClick={() => {
        api.permissionRequest(kind);
        onRequest?.();
      }}
    >
      {t("perm.grant")}
    </button>
  );
}

/** Drei Schritte mit einer nachgebauten Zeile aus den Systemeinstellungen. */
function AccessibilityGuide() {
  const { t } = useApp();
  return (
    <div className="perm-guide">
      <h3 className="row-label">{t("perm.guide_title")}</h3>
      <ol className="steps">
        <li>{t("perm.guide_open")}</li>
        <li>
          <span>
            {t("perm.guide_switch")}
            <span className="perm-mock" aria-hidden="true">
              <Bird size={22} />
              <span className="spacer">Whisper Bird</span>
              <span className="toggle on" />
            </span>
          </span>
        </li>
        <li>{t("perm.guide_back")}</li>
      </ol>
      <p className="row-hint">{t("perm.guide_missing")}</p>
    </div>
  );
}

export function PermissionList({ perms }: { perms: Permissions }) {
  const { t } = useApp();
  // macOS zeigt seine Abfrage nur einmal; danach geht es direkt in die Einstellungen.
  const [asked, setAsked] = useState(false);
  return (
    <Card>
      <Row label={t("perm.accessibility")} hint={t("perm.accessibility_hint")}>
        <Action kind="accessibility" granted={perms.accessibility} denied={asked} onRequest={() => setAsked(true)} />
      </Row>
      {asked && !perms.accessibility && <AccessibilityGuide />}
      <Row label={t("perm.microphone")} hint={t("perm.microphone_hint")}>
        <Action kind="microphone" granted={perms.microphone === "granted"} denied={perms.microphone === "denied"} />
      </Row>
    </Card>
  );
}
