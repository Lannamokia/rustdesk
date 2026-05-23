<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code>-fork med VHD-maskingodkendelsesbroen</b><br>
  <a href="#fork-status">Fork-status</a> &bull;
  <a href="#hvad-denne-fork-tilføjer">Tilføjelser</a> &bull;
  <a href="#bygning">Bygning</a> &bull;
  <a href="#hemmeligheder-og-ci">Hemmeligheder &amp; CI</a> &bull;
  <a href="#licens-og-attribution">Licens</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Dette repository er en downstream-fork af [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Fuld engelsk dokumentation: [`../README.md`](../README.md).
> Upstreams ophavsret, varemærker og AGPL-3.0-licens er uændrede &mdash; se [Licens og attribution](#licens-og-attribution).

> [!Caution]
> **Misbrugsforbehold:** upstream RustDesk-udviklerne og vedligeholderne af denne fork accepterer eller støtter ikke uetisk eller ulovlig brug af denne software. Uautoriseret adgang, kontrol eller krænkelse af privatlivet er strengt forbudt. Forfatterne er ikke ansvarlige for eventuel misbrug.

---

## Fork-status

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (i git som `upstream`-remote) |
| **Denne fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktiv branch** | `feature/vhd-machine-auth-bridge` |
| **Submodul** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), samme branch |
| **Licens** | AGPL-3.0 (uændret fra upstream &mdash; se [`LICENCE`](../LICENCE)) |
| **Mål** | At køre RustDesks Controlled-side som sidecar til den eksterne VHDMount-agent via en autentificeret, maskinebundet bro. |

Når `vhd-bridge` er **slået fra**, opfører byggeartefaktet sig identisk med upstream RustDesk &mdash; invariansen tjekkes automatisk af `tests/feature_off_parity.rs`.

## Hvad denne fork tilføjer

Et sammenhængende undersystem &mdash; **VHD-maskingodkendelsesbroen** &mdash; styret af to Cargo-features, der **er slået fra som standard**:

- **`vhd-bridge`** &mdash; kompilerer brokens worker, IPC-kabling, vedligeholdelses-overlay-UI og smoke-tests ind.
- **`controlled-only`** &mdash; fjerner Controller-(initiator-)UI og -kodestier, så binæren kun kan styres; kombineres med `vhd-bridge` til produktions-sidecar-builds.

Uden aktive features kører `cargo run` og upstream-build-flowet uændret.

### Vigtigste ændringer

- **`src/vhd_bridge/`** &ndash; named-pipe-worker, tilstandsmaskinen `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 med 32-byte delt hemmelighed indsprøjtet ved build, reconnect-backoff, struktureret observerbarhed, log sink der redacter hemmeligheder.
- **`src/server/connection.rs`** &ndash; godkendelsesport: før en indgående peer accepteres, konsulteres maskingodkendelses-peer-sættet, som broen vedligeholder.
- **`src/auth_2fa.rs`** &ndash; 2FA tvinges OFF, så længe broen styrer godkendelsen (verificeret af `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay der afspejler broens tilstand (`active / starting / lost`).
- **`libs/build_support/`** &ndash; hjælpe-crate delt mellem `build.rs` og CI: streng forudsætningsport, tolerant `secret.sec`-parser, konsistenstest mod protokoldokumentet.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; wire-protokolreference.
- **`scripts/check_bridge_strings.ps1`** &ndash; post-build-lækscanner: garanterer at `HBBS Key` / `VHDMount Key` aldrig lækker som klartekst i artefakter.
- **`.github/workflows/vhd-bridge.yml`** &mdash; CI-matrix der bygger feature-on / feature-off / controlled-only Windows-artefakter.

Fuld specifikation i [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klon

Forken ændrer URL'en for `libs/hbb_common`-submodulet; klon rekursivt:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Hvis du tidligere har klonet med upstream-`.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Bygning

### Upstream-build (uden bro)

Uden aktive features er forken en streng overmængde af upstream; upstream-instruktionerne gælder uændret. Fulde afhængigheder og kommandoer i [`../README.md`](../README.md).

### Build med bro (Windows MSVC, anbefalet)

Broen understøtter aktuelt kun Windows (named-pipe-transport og VHDMount-agent).

Krævede miljøvariabler:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <sti til LLVM\x64\bin>
```

Udfyld derefter dev-only `secret.sec` eller sæt de tilsvarende miljøvariabler, og:

```sh
# Produktions-sidecar (bro ON, controller fjernet)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Kun bro (controller-UI bevaret til dev)
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

Sidste kørsel på denne branch: 0 fejl / 189 unit / 6 + 8 integration / 38 + 4 build_support.

## Hemmeligheder og CI

Broen kræver fem build-time-input:

| Variabel | Formål | Format |
|---|---|---|
| `HBBS_KEY` | Rendezvous-serverens offentlige nøgle (overskriver `RS_PUB_KEY`) | base64, 32 bytes efter dekodning |
| `HBBS_HOST` | Rendezvous-serverhost | `host[:port[-port2]]` |
| `HBBR_HOST` | Relay-serverhost | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (eller `_B64`) | 32-byte HMAC-delt hemmelighed | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monoton nøglerotationsversion | ikke-negativt heltal |

To veje:

1. **Lokal udvikling** &mdash; udfyld `secret.sec` i repo-roden med `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Filen ignoreres af [`.gitignore`](../.gitignore).
2. **CI** &mdash; opsæt samme navne som GitHub Actions-repo-hemmeligheder; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) injicerer dem som maskerede env-variabler. **`secret.sec` materialiseres aldrig på runners**.

`secret.sec` og `vhd_bridge_secret.bin` ligger begge i `.gitignore` og må **aldrig committes**. `scripts/check_bridge_strings.ps1` er sikkerhedsnettet efter build.

## Licens og attribution

Forken distribueres under samme licens som upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Fuld tekst i [`../LICENCE`](../LICENCE); forken **ændrer ikke** licensen.

- Al ophavsret til upstream RustDesks kodebase tilhører upstream-forfatterne og bidragyderne, se <https://github.com/rustdesk/rustdesk>.
- Ændringer indført af denne fork (features `vhd-bridge` / `controlled-only` og tilhørende kode) distribueres ligeledes under AGPL-3.0; downstream-brugere bevarer alle rettigheder, AGPL-3.0 giver, herunder retten til den tilsvarende kildekode for enhver netværksudrulning.
- Navnet og logoet "RustDesk" tilhører upstream-projektet; forken bruger dem udelukkende til at identificere den modificerede kodebase, i overensstemmelse med fair brug af varemærker for fork-projekter af fri software.
- Tredjepartsbiblioteker (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter-afhængigheder) bevarer deres oprindelige licenser.

Brug af forken indebærer accept af AGPL-3.0 og **misbrugsforbeholdet** øverst i filen.
