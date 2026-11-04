# Whisper Bird auf dem Mac installieren

Die Mac-Version ist eine **Vorschau** für Macs mit Apple-Chip (M1 und neuer) ab macOS 12.
Sie ist noch nicht von Apple notarisiert. Deshalb sind beim ersten Start ein paar Klicks
mehr nötig als bei anderen Apps. Das dauert etwa zwei Minuten und ist nur einmal nötig.

## 1. Herunterladen und installieren

1. Die Datei `Whisper Bird_…_aarch64.dmg` herunterladen.
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

Ab jetzt startet Whisper Bird wie jede andere App.

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

## Gut zu wissen

- **Nach einem Update** kann macOS die Freigabe für die Bedienungshilfen zurücksetzen. Die App
  zeigt das oben als Hinweis an; ein Klick auf **Einstellungen öffnen**, Schalter wieder
  einschalten.
- **Updates** installiert die Mac-Version noch nicht selbst. Die App meldet eine neue Version
  und führt zur Download-Seite.
- **Shortcuts reagieren nicht?** Solange ein Passwortfeld oder im Terminal die „Sichere
  Tastatureingabe“ aktiv ist, blockiert macOS alle Shortcuts. Feld verlassen, dann geht es
  wieder.
- **Intel-Macs** werden nicht offiziell unterstützt; die Spracherkennung liefe dort nur auf der
  CPU.

## Daten und Deinstallation

| Was | Wo |
|---|---|
| Einstellungen, Verlauf, Modelle, Meetings | `~/Library/Application Support/dev.emil.whisper-island` |
| Logs (bei Problemen bitte mitschicken) | `~/Library/Logs/dev.emil.whisper-island` |

Zum Entfernen Whisper Bird aus **Programme** in den Papierkorb legen und den Ordner unter
*Application Support* löschen. Die Einträge unter Bedienungshilfen und Mikrofon lassen sich in
den Systemeinstellungen mit **−** entfernen.
