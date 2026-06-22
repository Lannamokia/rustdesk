<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> s mostem strojové autentizace VHD</b><br>
  <a href="#stav-forku">Stav forku</a> &bull;
  <a href="#co-tento-fork-přidává">Doplňky</a> &bull;
  <a href="#sestavení">Sestavení</a> &bull;
  <a href="#tajnosti-a-ci">Tajnosti &amp; CI</a> &bull;
  <a href="#licence-a-uvedení">Licence</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Tento repozitář je downstream fork [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Plná anglická dokumentace: [`../README.md`](../README.md).
> Autorská práva, ochranné známky a licence AGPL-3.0 upstreamu zůstávají nezměněny &mdash; viz [Licence a uvedení](#licence-a-uvedení).

> [!Caution]
> **Upozornění o zneužití:** vývojáři upstreamového RustDesku ani správci tohoto forku netolerují ani nepodporují žádné neetické či nezákonné použití tohoto softwaru. Neautorizovaný přístup, ovládání nebo narušení soukromí jsou striktně zakázány. Autoři neodpovídají za žádné zneužití aplikace.

---

## Stav forku

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (v gitu jako remote `upstream`) |
| **Tento fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktivní větev** | `feature/vhd-machine-auth-bridge` |
| **Submodul** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), stejná větev |
| **Licence** | AGPL-3.0 (beze změn vůči upstreamu &mdash; viz [`LICENCE`](../LICENCE)) |
| **Cíl** | Provozovat ovládanou stranu RustDesku jako sidecar externího agenta VHDMount přes autentizovaný most přivázaný ke stroji. |

Když je `vhd-bridge` **vypnutý**, výsledný artefakt je chováním ekvivalentní upstreamovému RustDesku &mdash; tento invariant je automaticky ověřován testem `tests/feature_off_parity.rs`.

## Co tento fork přidává

Jeden soudržný podsystém &mdash; **most strojové autentizace VHD** &mdash; řízený dvěma Cargo features, **standardně vypnutými**:

- **`vhd-bridge`** &mdash; do binárky vkládá worker mostu, IPC propojení, UI overlay údržby a smoke testy.
- **`controlled-only`** &mdash; odstraňuje UI a kódové cesty Controlleru (initiatoru); spolu s `vhd-bridge` slouží pro produkční sidecar build.

Bez aktivních features se `cargo run` a upstream tok sestavování chovají identicky.

### Hlavní změny

- **`src/vhd_bridge/`** &ndash; worker pro pojmenovanou rouru, stavový automat `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 nad 32bajtovým sdíleným tajemstvím vloženým při buildu, backoff reconnectů, strukturovaná observabilita, log sink redigující tajné údaje.
- **`src/server/connection.rs`** &ndash; brána schvalování: před přijetím peer připojení se konzultuje machine-auth peer set udržovaný mostem.
- **`src/auth_2fa.rs`** &ndash; 2FA je vynucené OFF, dokud most řídí autentizaci (ověřeno `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay odrážející stav mostu (`active / starting / lost`).
- **`libs/build_support/`** &ndash; pomocná crate sdílená mezi `build.rs` a CI: striktní brána prerekvizit, tolerantní parser `secret.sec`, test konzistence s dokumentací protokolu.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; reference drátového protokolu.
- **`scripts/check_bridge_strings.ps1`** &ndash; post-build skener úniků: zaručuje, že žádné plain-textové bajty `HBBS Key` / `VHDMount Key` neuniknou do artefaktů.
- **`.github/workflows/build.yml`** &mdash; cross-platform CI workflow; key Windows jobs are **controller-windows** (Flutter desktop bundle, default features + `hwcodec` + `vram` + `flutter`, no bridge) and **controlled-windows** (controlled sidecar, `--features vhd-bridge,controlled-only,hwcodec,vram`); the leakage + smoke scripts run there too.

Plná specifikace v [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klonování

Fork mění URL submodulu `libs/hbb_common`, klonujte rekurzivně:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Pokud jste klonovali s upstreamovým `.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Sestavení

### Upstream build (bez mostu)

Bez aktivních features je tento fork striktní nadmnožinou upstreamu; upstream návody platí beze změn. Plné závislosti a příkazy v [`../README.md`](../README.md).

### Build s mostem (Windows MSVC, doporučeno)

Most aktuálně podporuje pouze Windows (named-pipe transport a agent VHDMount).

Vyžadované prostředí:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <cesta k LLVM\x64\bin>
```

Pak vyplňte dev-only `secret.sec` nebo nastavte odpovídající env proměnné a:

```sh
# Produkční sidecar build (most zapnut, controller odstraněn)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# Pouze most (UI controlleru zachováno pro dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Ověření

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Poslední běh na této větvi: 0 chyb / 189 unit / 6 + 8 integračních / 38 + 4 build_support.

## Tajnosti a CI

Most vyžaduje pět vstupů v době sestavení:

| Proměnná | Účel | Formát |
|---|---|---|
| `HBBS_KEY` | Veřejný klíč rendezvous serveru (přepisuje `RS_PUB_KEY`) | base64, po dekódování 32 bajtů |
| `HBBS_HOST` | Host rendezvous serveru | `host[:port[-port2]]` |
| `HBBR_HOST` | Host relay serveru | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (nebo `_B64`) | 32bajtové sdílené HMAC tajemství | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monotonně rostoucí verze rotace klíče | nezáporné celé číslo |

Dvě cesty:

1. **Lokální vývoj** &mdash; vyplnit `secret.sec` v rootu repozitáře řádky `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Soubor je ignorován [`.gitignore`](../.gitignore).
2. **CI** &mdash; nastavit stejnojmenné repository secrets v GitHub Actions; [`.github/workflows/build.yml`](../.github/workflows/build.yml) je injektuje jako maskované env proměnné. **`secret.sec` se na runnerech nikdy nematerializuje**.

`secret.sec` i `vhd_bridge_secret.bin` jsou v `.gitignore` a **nikdy nesmí být commitovány**. `scripts/check_bridge_strings.ps1` je záchranná síť po sestavení.

## Licence a uvedení

Tento fork je distribuován pod stejnou licencí jako upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Plný text v [`../LICENCE`](../LICENCE); fork licenci **neupravuje**.

- Veškerá autorská práva ke kódu upstreamového RustDesku zůstávají autorům a přispěvatelům upstreamu, viz <https://github.com/rustdesk/rustdesk>.
- Změny zavedené tímto forkem (features `vhd-bridge` / `controlled-only` a podpůrný kód) jsou rovněž distribuovány pod AGPL-3.0; downstream uživatelé si zachovávají všechna práva udělená AGPL-3.0, včetně práva na odpovídající zdrojový kód při síťovém nasazení.
- Název a logo "RustDesk" patří upstream projektu; fork je používá výhradně k identifikaci upravované kódové báze v rámci poctivého použití ochranných známek u forků svobodného softwaru.
- Knihovny třetích stran (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; závislosti Flutteru) si ponechávají své původní licence.

Použitím forku souhlasíte s podmínkami AGPL-3.0 a s **upozorněním o zneužití** v záhlaví souboru.
