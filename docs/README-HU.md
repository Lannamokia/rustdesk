<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code> fork a VHD-gép-hitelesítő híddal</b><br>
  <a href="#fork-állapot">Fork állapot</a> &bull;
  <a href="#mit-ad-hozzá-ez-a-fork">Bővítések</a> &bull;
  <a href="#fordítás">Fordítás</a> &bull;
  <a href="#titkok-és-ci">Titkok &amp; CI</a> &bull;
  <a href="#licenc-és-attribúció">Licenc</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Ez a tárhely a [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) downstream forkja. Teljes angol dokumentáció: [`../README.md`](../README.md).
> Az upstream szerzői jog, védjegyek és AGPL-3.0 licenc változatlanul érvényes &mdash; lásd [Licenc és attribúció](#licenc-és-attribúció).

> [!Caution]
> **Visszaélési nyilatkozat:** az upstream RustDesk fejlesztői és e fork karbantartói nem tűrik és nem támogatják a szoftver bármilyen etikátlan vagy illegális használatát. A jogosulatlan hozzáférés, vezérlés vagy adatvédelmi sérelem szigorúan tilos. A szerzők nem felelnek visszaélésért.

---

## Fork állapot

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (gitben `upstream` remote) |
| **Ez a fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktív ág** | `feature/vhd-machine-auth-bridge` |
| **Almodul** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), ugyanazon ágon |
| **Licenc** | AGPL-3.0 (változatlan az upstreamhez képest &mdash; lásd [`LICENCE`](../LICENCE)) |
| **Cél** | A RustDesk vezérelt oldalának sidecar-szerű futtatása a külső VHDMount ügynök mellett, hitelesített, géphez kötött hídon keresztül. |

Amikor a `vhd-bridge` **kikapcsolva**, a build artifact viselkedésében megegyezik az upstream RustDesk-kel &mdash; ezt a `tests/feature_off_parity.rs` automatikusan ellenőrzi.

## Mit ad hozzá ez a fork

Egyetlen koherens alrendszert &mdash; a **VHD-gép-hitelesítő hidat** &mdash;, két Cargo-feature-rel, amelyek **alapból ki vannak kapcsolva**:

- **`vhd-bridge`** &mdash; befordítja a híd workert, IPC-vezetékeket, karbantartási overlay UI-t és smoke-teszteket.
- **`controlled-only`** &mdash; eltávolítja a Controller (initiator) UI-t és kódutakat, így a binary csak vezérelhető marad; a `vhd-bridge`-vel kombinálva produkciós sidecar buildet eredményez.

Aktív feature-ök nélkül a `cargo run` és az upstream build folyamat változatlanul működik.

### Fő változások

- **`src/vhd_bridge/`** &ndash; named-pipe worker, állapotgép `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 a fordításkor injektált 32 bájtos megosztott titokkal, újrakapcsolódási backoff, strukturált observability, titkokat redaktáló log sink.
- **`src/server/connection.rs`** &ndash; jóváhagyási kapu: bejövő peer elfogadása előtt a híd által karbantartott machine-auth peer-halmazt egyezteti.
- **`src/auth_2fa.rs`** &ndash; a 2FA kötelezően OFF, amíg a híd irányítja az autentikációt (igazolja a `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay a híd állapotának (`active / starting / lost`) megjelenítésére.
- **`libs/build_support/`** &ndash; segédcrate, amelyet a `build.rs` és a CI is használ: szigorú előfeltétel-kapu, toleráns `secret.sec` parser, protokoll-doc-konzisztencia teszt.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; vezeték protokoll referencia.
- **`scripts/check_bridge_strings.ps1`** &ndash; build utáni szivárgás-szkenner: garantálja, hogy `HBBS Key` / `VHDMount Key` nyílt szövegű bájtjai ne kerüljenek a buildbe.
- **`.github/workflows/vhd-bridge.yml`** &mdash; CI mátrix, amely feature-on / feature-off / controlled-only Windows-artifactokat épít.

Teljes specifikáció: [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klónozás

A fork módosítja a `libs/hbb_common` almodul URL-jét, ezért rekurzívan klónozz:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Ha korábban az upstream `.gitmodules`-szal klónoztál: `git submodule sync && git submodule update --init --recursive`.

## Fordítás

### Upstream build (híd nélkül)

Aktív feature-ök nélkül a fork az upstream szigorú szuperhalmaza; az upstream útmutató változatlanul érvényes. Teljes függőségek és parancsok: [`../README.md`](../README.md).

### Build híddal (Windows MSVC, ajánlott)

A híd jelenleg csak Windowst támogat (named-pipe transport és VHDMount ügynök).

Szükséges környezet:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <LLVM\x64\bin elérési útja>
```

Ezután töltsd ki a dev-only `secret.sec`-et vagy állítsd be a megfelelő env változókat, majd:

```sh
# Produkciós sidecar build (híd ON, controller eltávolítva)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Csak híd (controller UI fennmarad fejlesztéshez)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Ellenőrzés

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Legutóbbi futás ezen az ágon: 0 hiba / 189 unit / 6 + 8 integrációs / 38 + 4 build_support.

## Titkok és CI

A híd öt build-időbeli bemenetet vár:

| Változó | Cél | Formátum |
|---|---|---|
| `HBBS_KEY` | A rendezvous szerver publikus kulcsa (felülírja `RS_PUB_KEY`) | base64, dekódolva 32 bájt |
| `HBBS_HOST` | Rendezvous szerver host | `host[:port[-port2]]` |
| `HBBR_HOST` | Relay szerver host | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (vagy `_B64`) | 32 bájtos megosztott HMAC titok | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monoton növekvő rotációs verzió | nemnegatív egész |

Két út:

1. **Helyi fejlesztés** &mdash; töltsd ki a `secret.sec`-et a repó gyökerében: `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. A fájl a [`.gitignore`](../.gitignore) miatt nem kerül verziókezelés alá.
2. **CI** &mdash; ugyanezeket a neveket állítsd be GitHub Actions repository secret-ként; a [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) maszkolt env változókként injektálja. **A `secret.sec` soha nem materializálódik a runnereken**.

A `secret.sec` és a `vhd_bridge_secret.bin` is `.gitignore`-ban van és **soha nem szabad** commitolni. A `scripts/check_bridge_strings.ps1` a build utáni biztonsági háló.

## Licenc és attribúció

A fork ugyanazon licenc alatt érhető el, mint az upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Teljes szöveg: [`../LICENCE`](../LICENCE); a fork **nem módosítja** a licencet.

- Az upstream RustDesk kódbázis minden szerzői joga az upstream szerzőké és közreműködőké marad, lásd <https://github.com/rustdesk/rustdesk>.
- A fork által bevezetett módosítások (`vhd-bridge` / `controlled-only` feature-ök és a kapcsolódó kód) szintén AGPL-3.0 alatt kerülnek terjesztésre; a downstream felhasználók megőrzik az AGPL-3.0 által biztosított minden jogot, beleértve a hálózati telepítéshez tartozó forráskód-igénylési jogot.
- A "RustDesk" név és logó az upstream projekt tulajdona; a fork csak a módosított kódbázis azonosítására használja, a szabad szoftver forkok védjegyhasználatának korrekt gyakorlatának megfelelően.
- Harmadik féltől származó könyvtárak (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter függőségek) eredeti licencüket megőrzik.

A fork használata az AGPL-3.0 és a fájl tetején lévő **visszaélési nyilatkozat** elfogadását jelenti.
