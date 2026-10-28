# Whisper Bird

Lokales Diktieren, Verbessern und Transkribieren für Windows – in jede App.
Rust-Backend (Tauri 2) mit React/TypeScript-Oberfläche.

## Funktionen

| Bereich | Was es kann |
|---|---|
| **Diktat** | Globaler Shortcut (Standard `Strg+Umschalt+Leertaste`), Umschalten / Push-to-talk / Hybrid, Text erscheint an der Cursorposition. Esc oder ✕ bricht ab. |
| **Overlay** | Verschiebbares Floating Icon (Klick = Diktat, Rechtsklick = Menü, Dateien darauf ziehen = Transkription). Während der Aufnahme eine Pille mit Live-Waveform. Nimmt nie den Fokus. |
| **Modelle** | Parakeet v3 (Standard), Qwen3-ASR 1.7B/0.6B, Canary 1B v2, Parakeet English, Whisper Tiny bis Large v3 – lokal über transcribe.cpp mit Vulkan-GPU. |
| **Enhance** | Grammatik, Zeichensetzung, Füllwörter, Selbstkorrekturen, app-gerechte Formatierung (E-Mail, Chat, Code …). Lokal mit Gemma 4 (llama.cpp), per CLI (Claude Code, Codex, Gemini) oder OpenAI-kompatibler API. Immer an oder über einen zweiten Shortcut. Fällt bei Fehlern auf den Rohtext zurück. |
| **Text bearbeiten** | Markierten Text in jeder App per Shortcut verbessern, kürzen, erweitern, Ton ändern, als Liste formatieren, übersetzen – oder eigene Anweisung tippen bzw. sprechen. |
| **Meetings** | Mikrofon und/oder PC-Audio (WASAPI-Loopback), Live-Transkript, danach Sprechererkennung (Sortformer), umbenennbare Sprecher, automatische Notizen, Export als PDF oder Text. |
| **Dateien** | Audio/Video oder ganze Ordner per Drag & Drop – eine Textdatei pro Aufnahme, im Hintergrund mit Fortschritt. |
| **Wörter** | Custom Words (unscharfe Korrektur und Modell-Vokabular), Snippets (gesprochener Auslöser → fester Text). |
| **Verlauf & Statistik** | Suche, Filter nach Quelle und Datum, Doppelklick kopiert; Wörter, Dauer, gesparte Zeit, Sprachen, 30-Tage-Verlauf. |
| **Komfort** | Zwischenablage bleibt erhalten, Auto-Enter pro App, Töne, Audio-Ducking, Tray-Menü, Autostart, DE/EN-Oberfläche. |

## Entwickeln

Voraussetzungen (einmalig, z. B. per `winget`): Rust, Bun, CMake, LLVM, Vulkan SDK,
Visual Studio Build Tools mit C++.

```powershell
. .\scripts\env.ps1     # PATH, VULKAN_SDK und LIBCLANG_PATH laden
bun install
bun run tauri dev       # Entwicklungsmodus
bun run tauri build     # Installer (NSIS) unter src-tauri\target\release\bundle
```

Der erste Build dauert rund 10 Minuten (transcribe.cpp mit Vulkan-Shadern).
Tests: `cd src-tauri; cargo test --lib`.

Icons neu erzeugen: `bun run icons`.

## Exe per GitHub Actions

Der Workflow `.github/workflows/build.yml` baut auf `windows-latest` den
NSIS-Installer und eine portable Zip (Exe + VC++-Runtime-DLLs):

- **Manuell:** `gh workflow run build.yml` (oder Actions → Build → *Run workflow*),
  die Dateien hängen danach als Artifact am Lauf (`gh run download`).
- **Release:** `git tag v0.2.0 && git push origin v0.2.0` – baut mit dieser
  Versionsnummer und legt ein GitHub Release mit beiden Dateien an.

Die Builds sind nicht signiert (SmartScreen-Hinweis beim ersten Start). Die Exe
braucht einen Vulkan-fähigen Grafiktreiber (`vulkan-1.dll`), den aktuelle
AMD-, NVIDIA- und Intel-Treiber mitbringen.

## Aufbau

```
src-tauri/src/
  lib.rs          Start, Plugins, Fenster, Hintergrundaufgaben
  dictation.rs    Ablauf Aufnahme → Transkription → Enhance → Einfügen
  hotkeys.rs      Globale Shortcuts (handy-keys), Shortcut-Aufnahme
  asr.rs          transcribe-cpp: Modell laden, VAD-Zerlegung langer Audios
  enhance/        Prompts, llama-server-Verwaltung, CLI- und API-Anbieter
  editmenu.rs     Kontextmenü „Text bearbeiten“
  meeting.rs      Meeting-Aufnahme, Live-Transkript, Diarisierung, Notizen
  files.rs        Datei-/Ordner-Transkription
  insert.rs       Einfügen und Markierung auslesen
  win/            Zwischenablage, SendInput, aktive App
  audio/          Mikrofon, Loopback, Resampler, Decoder, Töne, Ducking
  overlay.rs      Floating Icon / Statuspille
  tray.rs         Tray- und Kontextmenü
  history.rs      Verlauf & Statistiken (SQLite)
src/
  App.tsx, pages/ Einstellungsfenster
  overlay/        Overlay-Fenster
  editmenu/       Kontextmenü-Fenster
```

Daten liegen unter `%APPDATA%\dev.emil.whisper-island` (Einstellungen, Verlauf,
Modelle, Meetings), Logs unter `%LOCALAPPDATA%\dev.emil.whisper-island\logs`.

## Danksagungen

Audioaufnahme, Hotkeys und Spracherkennung nutzen die MIT-lizenzierten Crates
`handy-recorder`, `handy-keys` und `transcribe-cpp` aus dem
[Handy](https://github.com/cjpais/Handy)-Projekt. Enhance läuft über
[llama.cpp](https://github.com/ggml-org/llama.cpp).
