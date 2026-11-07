# Whisper Bird auf dem Mac installieren

Die Mac-Version ist eine **Vorschau** für Macs mit Apple-Chip (M1 und neuer) ab macOS 12.
Sie ist noch nicht von Apple notarisiert. Deshalb sind beim ersten Start ein paar Klicks
mehr nötig als bei anderen Apps. Das dauert etwa zwei Minuten und kommt bei jedem
[Update](#auf-eine-neue-version-updaten) wieder.

## 1. Herunterladen und installieren

1. Unter [**Releases**](https://github.com/CodingEmil/whisper-bird/releases/latest) die Datei
   `Whisper.Bird_…_aarch64.dmg` herunterladen.
2. Die `.dmg` per Doppelklick öffnen und **Whisper Bird** in den Ordner **Programme** ziehen.
3. Das Fenster der `.dmg` schließen. Im Finder lässt sie sich links neben dem Laufwerksnamen
   auswerfen.

## 2. Den ersten Start erlauben

Beim ersten Öffnen zeigt macOS diesen Hinweis:

> **„Whisper Bird“ wurde nicht geöffnet.** Apple konnte nicht überprüfen, ob „Whisper Bird“
> frei von Schadsoftware ist, die deinen Mac beschädigen oder deine Privatsphäre verletzen kann.

Das liegt nur an der fehlenden Notarisierung. Whisper Bird läuft komplett lokal und schickt
keine Aufnahmen ins Internet. So erlaubst du den Start:

1. Im Hinweis auf **Fertig** klicken (nicht auf „In den Papierkorb legen“).
2. Das **Apple-Menü  → Systemeinstellungen** öffnen und links **Datenschutz & Sicherheit**
   wählen.
3. Rechts ganz nach unten scrollen bis zum Abschnitt **Sicherheit**. Dort steht
   *„Whisper Bird“ wurde blockiert, um deinen Mac zu schützen.*
4. Daneben auf **Trotzdem öffnen** klicken und mit Passwort oder Touch ID bestätigen.
5. Whisper Bird erneut aus **Programme** starten und im letzten Hinweis noch einmal
   **Trotzdem öffnen** wählen.

Ab jetzt startet diese Version wie jede andere App.

<details>
<summary><b>Schneller über das Terminal</b></summary>

<br>

Statt der Schritte oben reicht ein Befehl im Terminal (Programme → Dienstprogramme → Terminal):

```sh
xattr -dr com.apple.quarantine "/Applications/Whisper Bird.app"
```

Danach öffnet sich Whisper Bird ohne Hinweis.

</details>

## 3. Einrichtung in der App

Beim ersten Start führt Whisper Bird durch zwei Freigaben. Die App erklärt jeden Schritt selbst:

- **Bedienungshilfen**: nötig, damit deine Shortcuts funktionieren und Text in anderen Apps
  landet. Auf **Freigeben** klicken, in der Abfrage **Systemeinstellungen öffnen** wählen und
  den Schalter neben Whisper Bird einschalten. Whisper Bird kommt danach von selbst wieder nach
  vorn.
- **Mikrofon**: macOS fragt mit einem normalen Ja/Nein-Dialog.

Danach ein Spracherkennungsmodell wählen und laden, und los geht's:
<kbd>⌃</kbd> + <kbd>⇧</kbd> + <kbd>Leertaste</kbd> drücken, sprechen, nochmal drücken.

Für **Meetings mit System-Audio** (macOS 14.6 oder neuer) fragt macOS beim ersten Mal nach
*Nur Systemaudio aufnehmen*. Auch das einfach erlauben.

## Auf eine neue Version updaten

Die Mac-Version installiert Updates noch nicht selbst, du ersetzt die App von Hand. Einstellungen,
Verlauf und Modelle bleiben dabei erhalten, weil sie nicht in der App liegen.

1. **Neue Version finden:** Das Menü in der Menüleiste zeigt **Update verfügbar**. Unter
   **Hilfe & Info** führt **Herunterladen** zur Release-Seite. Dort die neue `.dmg` laden.
2. **App beenden:** Im Menü in der Menüleiste **Whisper Bird beenden** wählen. Erscheint dabei
   eine Absturzmeldung, siehe [Bekannte Probleme](#bekannte-probleme).
3. **App ersetzen:** Die `.dmg` öffnen, **Whisper Bird** in **Programme** ziehen und
   **Ersetzen** wählen. Die `.dmg` danach auswerfen.
4. **Start erlauben:** macOS blockiert auch die neue Version beim ersten Start. Das geht wie unter
   [Den ersten Start erlauben](#2-den-ersten-start-erlauben), am schnellsten mit dem Befehl im
   Terminal.
5. **Shortcuts wieder freigeben:** Nach dem Update kann die Freigabe für die Bedienungshilfen
   verloren sein. Dann helfen die Schritte im nächsten Abschnitt.

## Shortcuts funktionieren nach einem Update nicht

macOS erkennt die neue Version unter Umständen nicht mehr als dieselbe App. Der Schalter kann
dann eingeschaltet aussehen, ohne zu wirken. Whisper Bird zeigt in dem Fall oben den Hinweis
*Kein Zugriff auf die Bedienungshilfen*, und ein Klick auf den Vogel startet das Diktat weiterhin.

1. **Systemeinstellungen → Datenschutz & Sicherheit → Bedienungshilfen** öffnen.
2. **Whisper Bird** auswählen und mit **−** entfernen.
3. Mit **+** die App aus **Programme** neu hinzufügen und den Schalter einschalten.

Whisper Bird erkennt die Freigabe von selbst, ein Neustart ist nicht nötig. Das kann ein paar
Minuten dauern. Hilft auch das nicht, setzt dieser Befehl im Terminal die Freigabe nur für
Whisper Bird zurück. Danach die Schritte 2 und 3 wiederholen:

```sh
tccutil reset Accessibility dev.emil.whisper-island
```

## Bekannte Probleme

Stand: Version 0.2.1.

| Was passiert | Was du tun kannst |
|---|---|
| **Absturzmeldung beim Beenden**, solange ein Spracherkennungsmodell geladen ist. | Meldung schließen und die App bei Bedarf neu starten. Im Test blieben Einstellungen und Verlauf dabei erhalten. Behoben in der nächsten Version nach 0.2.1. |
| **Overlay springt in die Mitte**, wenn eine Aufnahme startet oder endet oder die App neu startet. Betrifft vor allem Positionen in der rechten oder unteren Bildschirmhälfte auf Retina-Displays. | In 0.2.1 gibt es dafür keine Lösung. Behoben in der nächsten Version nach 0.2.1: Nach dem Update den Vogel einmal neu an die gewünschte Stelle ziehen. |
| **Menü „Text bearbeiten“ öffnet nicht am Mauszeiger**, sondern dort, wo es zuletzt war. Betrifft Retina-Displays, wenn der Mauszeiger rechts oder unten liegt. | In 0.2.1 gibt es dafür keine Lösung. Behoben in der nächsten Version nach 0.2.1. |
| **Das erste Diktat einer neuen Version kann verzögert starten.** Das Mikrofon brauchte im Test dabei 5 bis 7 Sekunden, danach nur noch Millisekunden, auch nach einem Neustart der App. | Beim ersten Diktat nach dem Update ein paar Sekunden warten, bevor du sprichst. Fehlt der Anfang, ihn noch einmal sprechen. |

## Gut zu wissen

- **Shortcuts reagieren nicht, obwohl die Freigabe stimmt?** Solange ein Passwortfeld oder im
  Terminal die „Sichere Tastatureingabe“ aktiv ist, blockiert macOS alle Shortcuts. Feld
  verlassen, dann geht es wieder.
- **Diktat bleibt leer?** Unter **Systemeinstellungen → Datenschutz & Sicherheit → Mikrofon**
  prüfen, ob Whisper Bird eingeschaltet ist, und ob das Mikrofon stummgeschaltet ist. Steht im
  Log `exact digital silence`, kam vom Mikrofon gar kein Ton an.
- **Intel-Macs** werden nicht offiziell unterstützt; die Spracherkennung liefe dort nur auf der
  CPU.

## Daten und Deinstallation

| Was | Wo |
|---|---|
| Einstellungen, Verlauf, Modelle, Meetings | `~/Library/Application Support/dev.emil.whisper-island` |
| Logs (bei Problemen bitte mitschicken) | `~/Library/Logs/dev.emil.whisper-island` |
| Absturzberichte (bei Abstürzen bitte mitschicken) | `~/Library/Logs/DiagnosticReports/whisper-island-*.ips` |

Zum Entfernen Whisper Bird aus **Programme** in den Papierkorb legen und den Ordner unter
*Application Support* löschen. Die Einträge unter Bedienungshilfen und Mikrofon lassen sich in
den Systemeinstellungen mit **−** entfernen.
