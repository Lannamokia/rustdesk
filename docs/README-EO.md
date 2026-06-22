<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code>-forko kun la VHD-maŝin-aŭtentokontrola ponto</b><br>
  <a href="#stato-de-la-forko">Stato de la forko</a> &bull;
  <a href="#kion-tiu-ĉi-forko-aldonas">Aldonoj</a> &bull;
  <a href="#kompilado">Kompilado</a> &bull;
  <a href="#sekretoj-kaj-ci">Sekretoj &amp; CI</a> &bull;
  <a href="#permesilo-kaj-atribuo">Permesilo</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Tiu ĉi deponejo estas malsupranflua forko de [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Plena angla dokumentaro: [`../README.md`](../README.md).
> Kopirajtoj, varmarkoj kaj la AGPL-3.0-permesilo de la fonto restas neŝanĝitaj &mdash; vidu [Permesilo kaj atribuo](#permesilo-kaj-atribuo).

> [!Caution]
> **Malagnosko pri misuzo:** la fontaj programistoj de RustDesk kaj la prizorgantoj de tiu forko ne toleras nek subtenas iun ajn maletika aŭ kontraŭleĝa uzon de tiu programo. Senrajta aliro, regado aŭ privatecorompo estas striktaj malpermesoj. La aŭtoroj ne respondecas pri ajna misuzo.

---

## Stato de la forko

| | |
|---|---|
| **Fonto (upstream)** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (en git kiel `upstream`-foro) |
| **Tiu ĉi forko** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktiva branĉo** | `feature/vhd-machine-auth-bridge` |
| **Submodulo** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), sama branĉo |
| **Permesilo** | AGPL-3.0 (sen ŝanĝo de la fonto &mdash; vidu [`LICENCE`](../LICENCE)) |
| **Celo** | Funkciigi la kontrolatan flankon de RustDesk kiel sidecar de la ekstera VHDMount-agento per aŭtentigita, al-maŝine ligita ponto. |

Kiam `vhd-bridge` estas **malŝaltita**, la kompilita artefakto kondutas same kiel la fonta RustDesk &mdash; tiu invariableco estas aŭtomate kontrolata de `tests/feature_off_parity.rs`.

## Kion tiu ĉi forko aldonas

Unu kohera subsistemo &mdash; **la VHD-maŝin-aŭtentokontrola ponto** &mdash; regata de du Cargo-ecoj, **defaŭlte malŝaltitaj**:

- **`vhd-bridge`** &mdash; enkompilas la pontan worker, la IPC-konekton, la mantena UI-overlay kaj la smoke-testojn.
- **`controlled-only`** &mdash; forigas la UI kaj kodvojojn de la Kontrolilo (initiator), produktante nur-kontrolatan binaron; kombinata kun `vhd-bridge` por la produktada sidecar-buildo.

Sen aktivaj ecoj, `cargo run` kaj la fonta build-fluo funkcias senŝanĝe.

### Ĉefaj ŝanĝoj

- **`src/vhd_bridge/`** &ndash; worker por named pipe, statomaŝino `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 kun 32-bajta dividita sekreto enĵetita dum la buildo, rekoneksio backoff, strukturita observebleco, log sink kiu redaktas sekretojn.
- **`src/server/connection.rs`** &ndash; aproba pordego: antaŭ akceptado de envenanta peer, oni konsultas la maŝin-aŭtentigan peer-aron tenitan de la ponto.
- **`src/auth_2fa.rs`** &ndash; 2FA estas devige malŝaltita dum la ponto regas la aŭtentigon (kontrolite de `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay reflektanta la staton de la ponto (`active / starting / lost`).
- **`libs/build_support/`** &ndash; helpa crate kunhavata de `build.rs` kaj CI: severa antaŭkondiĉa pordego, tolera analizilo por `secret.sec`, koherec-testo kun la protokol-dokumento.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; referenco de la dratprotokolo.
- **`scripts/check_bridge_strings.ps1`** &ndash; postbuilda elfluo-skanilo: garantias ke neniaj klartekstaj bajtoj de `HBBS Key` / `VHDMount Key` elfluas en la artefaktojn.
- **`.github/workflows/build.yml`** &mdash; transplatforma CI-fluo; la ŝlosilaj Windows-laboroj estas **controller-windows** (Flutter-labortabla pakaĵo, defaŭltaj funkcioj + `hwcodec` + `vram` + `flutter`, sen ponto) kaj **controlled-windows** (kontrolata flankrigardanto, `--features vhd-bridge,controlled-only,hwcodec,vram`), kun ekzekuto de la elflu- kaj fum-skriptoj.

Plena specifo en [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Kloni

La forko ŝanĝas la URL de la submodulo `libs/hbb_common`; klonu rekursive:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Se vi jam klonis kun la fonta `.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Kompilado

### Fonta build (sen ponto)

Sen aktivaj ecoj, la forko estas strikta superaro de la fonto; la fontaj instrukcioj validas senŝanĝe. Plenaj dependoj kaj komandoj en [`../README.md`](../README.md).

### Build kun ponto (Windows MSVC, rekomendata)

La ponto nuntempe subtenas nur Windows (named-pipe-transporto kaj VHDMount-agento).

Bezonata medio:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <vojo al LLVM\x64\bin>
```

Tiam plenigu la dev-only `secret.sec` aŭ agordu la ekvivalentajn env-variantojn, poste:

```sh
# Produktada sidecar-build (ponto ŝaltita, controller forigita)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# Nur ponto (kontrolila UI konservata por dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Kontrolo

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Lasta plenumo sur tiu branĉo: 0 eraroj / 189 unit / 6 + 8 integraj / 38 + 4 build_support.

## Sekretoj kaj CI

La ponto bezonas kvin enirvalorojn dum la buildo:

| Variablo | Celo | Formato |
|---|---|---|
| `HBBS_KEY` | Publika ŝlosilo de la rendezvous-servilo (anstataŭas `RS_PUB_KEY`) | base64, 32 bajtoj post malkodado |
| `HBBS_HOST` | Servila gastiganto rendezvous | `host[:port[-port2]]` |
| `HBBR_HOST` | Servila gastiganto relay | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (aŭ `_B64`) | 32-bajta dividita HMAC-sekreto | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monotona ŝlosil-rotaciversio | nenegativa entjero |

Du vojoj:

1. **Loka disvolvado** &mdash; plenigu `secret.sec` en la radiko de la deponejo per la linioj `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. La dosiero estas ignorata de [`.gitignore`](../.gitignore).
2. **CI** &mdash; agordu la samajn nomojn kiel deponejajn sekretojn en GitHub Actions; [`.github/workflows/build.yml`](../.github/workflows/build.yml) injektas ilin kiel maskitajn env-variantojn. **`secret.sec` neniam materialiĝas sur runners**.

`secret.sec` kaj `vhd_bridge_secret.bin` ambaŭ estas en `.gitignore` kaj **neniam** estu komitataj. `scripts/check_bridge_strings.ps1` estas la sekureca reto post la buildo.

## Permesilo kaj atribuo

La forko estas distribuata sub la sama permesilo kiel la fonto: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Plena teksto en [`../LICENCE`](../LICENCE); la forko **ne ŝanĝas** la permesilon.

- Ĉiuj kopirajtoj de la fonta RustDesk-kodbazo restas ĉe la fontaj aŭtoroj kaj kontribuintoj, vidu <https://github.com/rustdesk/rustdesk>.
- La modifoj enkondukitaj de tiu forko (ecoj `vhd-bridge` / `controlled-only` kaj subtena kodo) estas same distribuataj sub AGPL-3.0; malsupranfluaj uzantoj retenas ĉiujn rajtojn donitajn de AGPL-3.0, inkluzive de la rajto al la respondiga fontkodo por iu ajn reta deplojo.
- La nomo kaj la emblemo "RustDesk" apartenas al la fonta projekto; la forko uzas ilin nur por identigi la modifitan kodbazon, laŭ la justa uzo de varmarkoj en forkoj de libera programaro.
- Triaj bibliotekoj (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter-dependoj) konservas siajn originajn permesilojn.

Uzi la forkon implicas akcepton de AGPL-3.0 kaj de la **malagnosko pri misuzo** ĉe la supro de la dosiero.
