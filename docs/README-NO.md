<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code>-fork med VHD-maskinautentiseringsbro</b><br>
  <a href="#fork-status">Fork-status</a> &bull;
  <a href="#hva-denne-forken-legger-til">Tillegg</a> &bull;
  <a href="#bygging">Bygging</a> &bull;
  <a href="#hemmeligheter-og-ci">Hemmeligheter &amp; CI</a> &bull;
  <a href="#lisens-og-attribusjon">Lisens</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Dette repoet er en downstream-fork av [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Full engelsk dokumentasjon: [`../README.md`](../README.md).
> Upstreams opphavsrett, varemerker og AGPL-3.0-lisens er uendret &mdash; se [Lisens og attribusjon](#lisens-og-attribusjon).

> [!Caution]
> **Misbruksforbehold:** upstream RustDesk-utviklerne og forkens vedlikeholdere aksepterer eller støtter ikke uetisk eller ulovlig bruk av denne programvaren. Uautorisert tilgang, kontroll eller personvernskrenkelse er strengt forbudt. Forfatterne er ikke ansvarlige for eventuelt misbruk.

---

## Fork-status

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (i git som `upstream`-remote) |
| **Denne forken** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktiv branch** | `feature/vhd-machine-auth-bridge` |
| **Undermodul** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), samme branch |
| **Lisens** | AGPL-3.0 (uendret fra upstream &mdash; se [`LICENCE`](../LICENCE)) |
| **Mål** | Kjøre RustDesks Controlled-side som sidecar til den eksterne VHDMount-agenten over en autentisert, maskinbundet bro. |

Når `vhd-bridge` er **av**, oppfører byggeartefaktet seg identisk med upstream RustDesk &mdash; invariansen verifiseres automatisk av `tests/feature_off_parity.rs`.

## Hva denne forken legger til

Ett sammenhengende undersystem &mdash; **VHD-maskinautentiseringsbroen** &mdash; styrt av to Cargo-features, **av som standard**:

- **`vhd-bridge`** &mdash; kompilerer inn broens worker, IPC-kabling, vedlikeholds-overlay-UI og smoke-tester.
- **`controlled-only`** &mdash; fjerner Controller-(initiator-)UI og kodebaner, slik at binæren kun kan styres; kombineres med `vhd-bridge` for produksjons-sidecar-bygg.

Uten aktive features kjører `cargo run` og upstream-byggeflyten uendret.

### Hovedendringer

- **`src/vhd_bridge/`** &ndash; named-pipe-worker, tilstandsmaskin `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 med 32-byte delt hemmelighet injisert ved bygg, reconnect-backoff, strukturert observerbarhet, log sink som redacter hemmeligheter.
- **`src/server/connection.rs`** &ndash; godkjenningsport: før en innkommende peer aksepteres, konsulteres maskinautentiserings-peer-settet broen vedlikeholder.
- **`src/auth_2fa.rs`** &ndash; 2FA tvinges OFF mens broen styrer autentisering (verifisert av `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay som reflekterer broens tilstand (`active / starting / lost`).
- **`libs/build_support/`** &ndash; hjelpercrate delt mellom `build.rs` og CI: streng forutsetningsport, tolerant `secret.sec`-parser, konsistenstest mot protokolldokumentet.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; wire-protokollreferanse.
- **`scripts/check_bridge_strings.ps1`** &ndash; post-build-lekkasjeskanner: garanterer at `HBBS Key` / `VHDMount Key` ikke lekker i klartekst i artefakter.
- **`.github/workflows/build.yml`** &mdash; tverrplattform CI-arbeidsflyt; de viktigste Windows-jobbene er **controller-windows** (Flutter desktop-bunt, standard features + `hwcodec` + `vram` + `flutter`, uten bridge) og **controlled-windows** (controlled sidecar, `--features vhd-bridge,controlled-only,hwcodec,vram`), og kjører også lekkasje- og smoke-skriptene.

Full spesifikasjon i [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klon

Forken endrer URL-en for `libs/hbb_common`-undermodulen; klon rekursivt:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Hvis du tidligere har klonet med upstream-`.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Bygging

### Upstream-bygg (uten bro)

Uten aktive features er forken en streng supermengde av upstream; upstream-instruksjonene gjelder uendret. Fulle avhengigheter og kommandoer i [`../README.md`](../README.md).

### Bygg med bro (Windows MSVC, anbefalt)

Broen støtter for tiden bare Windows (named-pipe-transport og VHDMount-agent).

Påkrevde miljøvariabler:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <sti til LLVM\x64\bin>
```

Fyll så ut dev-only `secret.sec` eller sett tilsvarende env-variabler, og:

```sh
# Produksjons-sidecar (bro PÅ, controller fjernet)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# Bare bro (controller-UI beholdt for dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verifikasjon

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Siste kjøring på denne branchen: 0 feil / 189 unit / 6 + 8 integrasjon / 38 + 4 build_support.

## Hemmeligheter og CI

Broen krever fem build-time-input:

| Variabel | Formål | Format |
|---|---|---|
| `HBBS_KEY` | Rendezvous-serverens offentlige nøkkel (overskriver `RS_PUB_KEY`) | base64, 32 bytes etter dekoding |
| `HBBS_HOST` | Rendezvous-serverhost | `host[:port[-port2]]` |
| `HBBR_HOST` | Relay-serverhost | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (eller `_B64`) | 32-byte HMAC delt hemmelighet | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monoton nøkkelrotasjonsversjon | ikke-negativt heltall |

To veier:

1. **Lokal utvikling** &mdash; fyll ut `secret.sec` i repo-roten med `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Filen ignoreres av [`.gitignore`](../.gitignore).
2. **CI** &mdash; sett opp samme navn som GitHub Actions-repo-hemmeligheter; [`.github/workflows/build.yml`](../.github/workflows/build.yml) injiserer dem som maskerte env-variabler. **`secret.sec` materialiseres aldri på runners**.

`secret.sec` og `vhd_bridge_secret.bin` ligger begge i `.gitignore` og må **aldri committes**. `scripts/check_bridge_strings.ps1` er sikkerhetsnettet etter bygg.

## Lisens og attribusjon

Forken distribueres under samme lisens som upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Full tekst i [`../LICENCE`](../LICENCE); forken **endrer ikke** lisensen.

- All opphavsrett til upstream RustDesks kodebase tilhører upstream-forfatterne og bidragsyterne, se <https://github.com/rustdesk/rustdesk>.
- Endringer innført av denne forken (features `vhd-bridge` / `controlled-only` og tilhørende kode) distribueres også under AGPL-3.0; downstream-brukere beholder alle rettigheter AGPL-3.0 gir, inkludert retten til den tilsvarende kildekoden for enhver nettverksdistribusjon.
- Navnet og logoen "RustDesk" tilhører upstream-prosjektet; forken bruker dem utelukkende for å identifisere den modifiserte kodebasen, i tråd med fair bruk av varemerker for fork-prosjekter av fri programvare.
- Tredjepartsbiblioteker (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter-avhengigheter) beholder sine opprinnelige lisenser.

Bruk av forken innebærer å akseptere AGPL-3.0 og **misbruksforbeholdet** øverst i filen.
