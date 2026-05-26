<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code>-fork met de VHD-machine-authbrug</b><br>
  <a href="#fork-status">Fork-status</a> &bull;
  <a href="#wat-deze-fork-toevoegt">Toevoegingen</a> &bull;
  <a href="#bouwen">Bouwen</a> &bull;
  <a href="#secrets-en-ci">Secrets &amp; CI</a> &bull;
  <a href="#licentie-en-naamsvermelding">Licentie</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Deze repo is een downstream-fork van [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Volledige Engelse documentatie: [`../README.md`](../README.md).
> Auteursrechten, merknamen en de AGPL-3.0-licentie van upstream blijven ongewijzigd &mdash; zie [Licentie en naamsvermelding](#licentie-en-naamsvermelding).

> [!Caution]
> **Misbruikvrijwaring:** de upstream-RustDesk-ontwikkelaars en de beheerders van deze fork dulden of ondersteunen geen onethisch of illegaal gebruik van deze software. Onbevoegde toegang, controle of privacy-inbreuk is strikt verboden. De auteurs zijn niet aansprakelijk voor enig misbruik.

---

## Fork-status

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (in git als `upstream`-remote) |
| **Deze fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Actieve branch** | `feature/vhd-machine-auth-bridge` |
| **Submodule** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), zelfde branch |
| **Licentie** | AGPL-3.0 (ongewijzigd t.o.v. upstream &mdash; zie [`LICENCE`](../LICENCE)) |
| **Doel** | De RustDesk-Controlled-zijde laten draaien als sidecar van de externe VHDMount-agent via een geauthenticeerde, machinegebonden brug. |

Wanneer `vhd-bridge` is **uitgeschakeld**, gedraagt het build-artefact zich identiek aan upstream-RustDesk &mdash; deze invariant wordt automatisch geverifieerd door `tests/feature_off_parity.rs`.

## Wat deze fork toevoegt

Eén samenhangend subsysteem &mdash; de **VHD-machine-authbrug** &mdash; aangestuurd door twee Cargo-features die **standaard uit** staan:

- **`vhd-bridge`** &mdash; compileert de brug-worker, IPC-bedrading, maintenance-overlay-UI en smoke-tests in.
- **`controlled-only`** &mdash; verwijdert Controller-(initiator-)UI en codepaden, zodat het binary alleen bestuurd kan worden; te combineren met `vhd-bridge` voor de productie-sidecar-build.

Zonder actieve features werken `cargo run` en de upstream-buildflow ongewijzigd.

### Belangrijkste wijzigingen

- **`src/vhd_bridge/`** &ndash; named-pipe-worker, toestandsmachine `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 met 32-byte gedeeld geheim dat tijdens de build wordt geïnjecteerd, herstel-backoff, gestructureerde observability, log sink die geheimen redigeert.
- **`src/server/connection.rs`** &ndash; goedkeuringspoort: voor het accepteren van een inkomende peer wordt de machine-auth-peerset van de brug geraadpleegd.
- **`src/auth_2fa.rs`** &ndash; 2FA wordt geforceerd uitgeschakeld zolang de brug authenticatie aanstuurt (geverifieerd door `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay die de brugstatus weergeeft (`active / starting / lost`).
- **`libs/build_support/`** &ndash; hulpcrate gedeeld door `build.rs` en CI: strikte vereisten-poort, tolerante `secret.sec`-parser, consistentie-test met de protocoldoc.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; wire-protocol-referentie.
- **`scripts/check_bridge_strings.ps1`** &ndash; post-build leak-scanner: garandeert dat geen klaartekstbytes van `HBBS Key` / `VHDMount Key` in de artefacten lekken.
- **`.github/workflows/build.yml`** &mdash; cross-platform CI-workflow; de belangrijkste Windows-jobs zijn **controller-windows** (Flutter desktop-bundle, default features + `hwcodec` + `vram` + `flutter`, zonder bridge) en **controlled-windows** (controlled sidecar, `--features vhd-bridge,controlled-only,hwcodec,vram`), die ook de leak- en smoke-scripts uitvoert.

Volledige spec in [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klonen

De fork wijzigt de submodule-URL voor `libs/hbb_common`; gebruik daarom recursive clone:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Heb je al gekloond met de upstream-`.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Bouwen

### Upstream-build (zonder brug)

Zonder actieve features is deze fork een strikte superset van upstream; de upstream-instructies gelden ongewijzigd. Volledige afhankelijkheden en commando's: [`../README.md`](../README.md).

### Build met brug (Windows MSVC, aanbevolen)

De brug ondersteunt momenteel alleen Windows (named-pipe-transport en VHDMount-agent).

Vereiste omgeving:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <pad naar LLVM\x64\bin>
```

Vul vervolgens dev-only `secret.sec` in of zet de bijbehorende env-vars en:

```sh
# Productie-sidecar (brug aan, controller verwijderd)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# Alleen brug (controller-UI behouden voor dev-iteraties)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verificatie

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Laatste run op deze branch: 0 fouten / 189 unit / 6 + 8 integratie / 38 + 4 build_support.

## Secrets en CI

De brug heeft vijf build-time-inputs nodig:

| Variabele | Doel | Formaat |
|---|---|---|
| `HBBS_KEY` | Publieke sleutel rendezvous-server (overschrijft `RS_PUB_KEY`) | base64, 32 bytes na decoding |
| `HBBS_HOST` | Host van rendezvous-server | `host[:port[-port2]]` |
| `HBBR_HOST` | Host van relay-server | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (of `_B64`) | 32-byte HMAC-geheim | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monotone sleutel-rotatieversie | niet-negatief geheel getal |

Twee paden:

1. **Lokale dev** &mdash; vul `secret.sec` in de repo-root met `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Bestand wordt genegeerd via [`.gitignore`](../.gitignore).
2. **CI** &mdash; gelijknamige GitHub-Actions-repo-secrets; [`.github/workflows/build.yml`](../.github/workflows/build.yml) injecteert ze als gemaskeerde env-variabelen. **`secret.sec` wordt nooit op runners gematerialiseerd**.

`secret.sec` en `vhd_bridge_secret.bin` staan beide in `.gitignore` en mogen **nooit gecommit worden**. `scripts/check_bridge_strings.ps1` is het vangnet na de build.

## Licentie en naamsvermelding

Deze fork wordt verspreid onder dezelfde licentie als upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Volledige tekst in [`../LICENCE`](../LICENCE); deze fork **wijzigt** de licentie niet.

- Alle auteursrechten op de upstream-RustDesk-codebase blijven bij de upstream-auteurs en -bijdragers; zie <https://github.com/rustdesk/rustdesk>.
- De wijzigingen in deze fork (features `vhd-bridge` / `controlled-only` met bijbehorende code) worden eveneens onder AGPL-3.0 verspreid; downstream-gebruikers behouden alle door AGPL-3.0 verleende rechten, inclusief het recht op de bijbehorende broncode bij elke netwerkimplementatie.
- De naam en het logo "RustDesk" zijn eigendom van het upstream-project; deze fork gebruikt ze uitsluitend om de aangepaste codebase te identificeren, conform fair use van merken bij forks van vrije software.
- Externe bibliotheken (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter-afhankelijkheden) behouden hun originele licenties.

Gebruik van deze fork betekent instemming met de AGPL-3.0 en de **misbruikvrijwaring** bovenaan dit bestand.
