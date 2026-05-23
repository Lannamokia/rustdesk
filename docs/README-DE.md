<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code> Fork mit der VHD-Maschinen-Auth-Bridge</b><br>
  <a href="#fork-status">Fork-Status</a> &bull;
  <a href="#was-dieser-fork-hinzufügt">Neuerungen</a> &bull;
  <a href="#bauen">Bauen</a> &bull;
  <a href="#geheimnisse-und-ci">Geheimnisse &amp; CI</a> &bull;
  <a href="#lizenz-und-zuschreibung">Lizenz</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Dieses Repository ist ein Downstream-Fork von [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Die vollständige englische Dokumentation findest du unter [`../README.md`](../README.md).
> Urheberrechte, Marken und die AGPL-3.0-Lizenz des Upstreams bleiben unverändert &mdash; siehe [Lizenz und Zuschreibung](#lizenz-und-zuschreibung).

> [!Caution]
> **Haftungsausschluss:** Die Upstream-RustDesk-Entwickler und die Wartenden dieses Forks dulden oder unterstützen keine unethische oder illegale Nutzung dieser Software. Missbrauch wie unerlaubter Zugriff, unerlaubte Steuerung oder Verletzung der Privatsphäre verstößt strikt gegen unsere Richtlinien. Die Autoren tragen keine Verantwortung für jegliche Form des Missbrauchs.

---

## Fork-Status

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (im Git als `upstream`-Remote eingetragen) |
| **Dieser Fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktiver Branch** | `feature/vhd-machine-auth-bridge` |
| **Submodul** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), gleicher Branch |
| **Lizenz** | AGPL-3.0 (unverändert vom Upstream &mdash; siehe [`LICENCE`](../LICENCE)) |
| **Ziel** | Die RustDesk-Controlled-Seite als Sidecar zum externen VHDMount-Agenten betreiben, über eine authentifizierte, maschinengebundene Bridge. |

Wenn `vhd-bridge` **deaktiviert** ist, ist das Build-Artefakt verhaltensgleich mit Upstream-RustDesk &mdash; diese Invariante wird durch `tests/feature_off_parity.rs` automatisch geprüft.

## Was dieser Fork hinzufügt

Ein zusammenhängendes Subsystem &mdash; die **VHD-Maschinen-Auth-Bridge** &mdash; gesteuert durch zwei Cargo-Features, die **standardmäßig deaktiviert** sind:

- **`vhd-bridge`** &mdash; kompiliert Bridge-Worker, IPC-Verdrahtung, Wartungs-Overlay-UI und Smoke-Tests ein.
- **`controlled-only`** &mdash; entfernt Controller-(Initiator-)UI und -Codepfade, damit die Binary ausschließlich gesteuert werden kann; in Kombination mit `vhd-bridge` für den produktiven Sidecar-Build vorgesehen.

Ohne aktive Features bleiben `cargo run` und der Upstream-Build-Flow unverändert.

### Wesentliche Änderungen

- **`src/vhd_bridge/`** &ndash; Named-Pipe-Worker, Zustandsmaschine `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 mit zur Build-Zeit eingespeistem 32-Byte-Geheimnis, Reconnect-Backoff, strukturierte Beobachtbarkeit, geheimnis-redigierender Log-Sink.
- **`src/server/connection.rs`** &ndash; Approval-Gate: prüft eingehende Peer-Verbindungen gegen das von der Bridge gepflegte Maschinen-Auth-Peer-Set, bevor sie akzeptiert werden.
- **`src/auth_2fa.rs`** &ndash; 2FA wird zwangsweise deaktiviert, solange die Bridge die Authentifizierung steuert (`tests/smoke_2fa_disabled.rs` verifiziert dies).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; Wartungs-Overlay, das den Bridge-Zustand (`active / starting / lost`) anzeigt.
- **`libs/build_support/`** &ndash; Hilfs-Crate, das `build.rs` und CI gemeinsam nutzen: strenger Voraussetzungs-Gate, toleranter `secret.sec`-Parser, Konsistenztest gegen die Protokoll-Doku.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; Wire-Protokoll-Referenz.
- **`scripts/check_bridge_strings.ps1`** &ndash; Post-Build-Leak-Scanner: stellt sicher, dass keine Klartext-`HBBS Key`- oder `VHDMount Key`-Bytes in den Artefakten landen.
- **`.github/workflows/vhd-bridge.yml`** &mdash; CI-Matrix, die feature-on / feature-off / controlled-only Windows-Artefakte baut.

Die vollständige Spezifikation liegt unter [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klonen

Der Fork ändert die Submodul-URL für `libs/hbb_common`, daher rekursiv klonen:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Wenn du vorher mit dem Upstream-`.gitmodules` geklont hast, führe `git submodule sync && git submodule update --init --recursive` aus.

## Bauen

### Upstream-Build (ohne Bridge)

Ohne aktive Features ist dieser Fork eine strikte Obermenge des Upstreams; die Upstream-Build-Anleitung gilt unverändert. Vollständige Abhängigkeiten und Befehle siehe [`../README.md`](../README.md).

### Bridge-Build (Windows MSVC, empfohlen)

Die Bridge unterstützt aktuell nur Windows (Named-Pipe-Transport und VHDMount-Agent).

Erforderliche Umgebung:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <Pfad zu LLVM\x64\bin>
```

Anschließend entweder die dev-only `secret.sec` ausfüllen oder die zugehörigen Variablen als Env setzen, dann:

```sh
# Produktiver Sidecar-Build (Bridge an, Controller entfernt)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Nur Bridge (Controller-UI für Dev-Iterationen behalten)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verifikation

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Letzter Lauf auf diesem Branch: 0 Fehler / 189 Unit-Tests / 6 + 8 Integrationstests / 38 + 4 build_support-Tests.

## Geheimnisse und CI

Die Bridge erwartet fünf Build-Zeit-Eingaben:

| Variable | Zweck | Format |
|---|---|---|
| `HBBS_KEY` | Public Key des RustDesk-Rendezvous-Servers (überschreibt `RS_PUB_KEY`) | base64, decodiert 32 Bytes |
| `HBBS_HOST` | Rendezvous-Server-Host | `host[:port[-port2]]` |
| `HBBR_HOST` | Relay-Server-Host | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (oder `_B64`) | 32-Byte-HMAC-Geheimnis | 64 Hex-Zeichen / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monoton wachsende Schlüssel-Rotationsversion | nicht-negative ganze Zahl |

Zwei Wege:

1. **Lokale Entwicklung** &mdash; im Repo-Root `secret.sec` mit den Zeilen `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:` anlegen. Datei ist via [`.gitignore`](../.gitignore) ignoriert.
2. **CI** &mdash; gleichnamige GitHub-Actions-Repository-Secrets unter `Settings &rarr; Secrets and variables &rarr; Actions`. [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) injiziert sie als maskierte Env-Variablen; **`secret.sec` wird auf CI-Runnern nie materialisiert**.

`secret.sec` und `vhd_bridge_secret.bin` stehen beide in `.gitignore` und dürfen **niemals committet** werden. `scripts/check_bridge_strings.ps1` ist das Auffangnetz nach dem Build.

## Lizenz und Zuschreibung

Dieser Fork wird unter derselben Lizenz wie der Upstream verteilt: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Vollständiger Text in [`../LICENCE`](../LICENCE), unverändert.

- Alle Urheberrechte am Upstream-RustDesk-Quellcode verbleiben bei den Upstream-Autoren und -Mitwirkenden, siehe <https://github.com/rustdesk/rustdesk>.
- Die in diesem Fork eingeführten Änderungen (`vhd-bridge` / `controlled-only` und Begleitcode) werden ebenfalls unter AGPL-3.0 verteilt; nachgelagerte Nutzer behalten alle von der AGPL-3.0 gewährten Rechte, einschließlich des Anspruchs auf den entsprechenden Quelltext bei Netzwerkbereitstellung.
- Name und Logo "RustDesk" gehören dem Upstream-Projekt; dieser Fork verwendet sie ausschließlich zur Kennzeichnung der modifizierten Codebasis im Sinne der Fair Use-Praxis bei Forks freier Software.
- Drittanbieter-Bibliotheken (vcpkg-installiert: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter-Abhängigkeiten) behalten ihre Originallizenzen.

Mit der Nutzung dieses Forks akzeptierst du die Bedingungen der AGPL-3.0 und den **Haftungsausschluss** am Anfang dieser Datei.
