<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code>-haarukka VHD-koneautentikointisillan kanssa</b><br>
  <a href="#haarukan-tila">Haarukan tila</a> &bull;
  <a href="#mitä-tämä-haarukka-lisää">Lisäykset</a> &bull;
  <a href="#kääntäminen">Kääntäminen</a> &bull;
  <a href="#salaisuudet-ja-ci">Salaisuudet &amp; CI</a> &bull;
  <a href="#lisenssi-ja-tekijätiedot">Lisenssi</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Tämä repo on alajuoksun haarukka projektista [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Täydellinen englanninkielinen dokumentaatio: [`../README.md`](../README.md).
> Yläjuoksun tekijänoikeudet, tavaramerkit ja AGPL-3.0-lisenssi pysyvät muuttumattomina &mdash; ks. [Lisenssi ja tekijätiedot](#lisenssi-ja-tekijätiedot).

> [!Caution]
> **Väärinkäyttöä koskeva vastuuvapauslauseke:** yläjuoksun RustDesk-kehittäjät ja tämän haarukan ylläpitäjät eivät hyväksy tai tue tämän ohjelmiston epäeettistä tai laitonta käyttöä. Luvaton pääsy, hallinta tai yksityisyyden loukkaaminen on ankarasti kielletty. Tekijät eivät vastaa väärinkäytöstä.

---

## Haarukan tila

| | |
|---|---|
| **Yläjuoksu** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (gitissä `upstream`-remoteena) |
| **Tämä haarukka** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktiivinen haara** | `feature/vhd-machine-auth-bridge` |
| **Alimoduuli** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), sama haara |
| **Lisenssi** | AGPL-3.0 (sama kuin yläjuoksulla &mdash; ks. [`LICENCE`](../LICENCE)) |
| **Tavoite** | Ajaa RustDeskin hallittua puolta sidecarina ulkoiselle VHDMount-agentille autentikoidun, koneeseen sidotun sillan kautta. |

Kun `vhd-bridge` on **pois käytöstä**, käännösartefakti on käytökseltään identtinen yläjuoksun RustDeskin kanssa &mdash; muuttumattomuuden varmistaa automaattisesti `tests/feature_off_parity.rs`.

## Mitä tämä haarukka lisää

Yhden yhtenäisen alijärjestelmän &mdash; **VHD-koneautentikointisilta** &mdash; jota ohjataan kahdella Cargo-featurella, **oletuksena pois päältä**:

- **`vhd-bridge`** &mdash; kääntää mukaan sillan workerin, IPC-kytkennän, ylläpidon overlay-UI:n ja smoke-testit.
- **`controlled-only`** &mdash; poistaa Controllerin (initiator) UI:n ja koodipolut tuottaen vain-hallittavan binäärin; yhdistetään `vhd-bridge`:n kanssa tuotanto-sidecar-buildissa.

Ilman aktiivisia featureita `cargo run` ja yläjuoksun build-virta toimivat muuttumattomina.

### Tärkeimmät muutokset

- **`src/vhd_bridge/`** &ndash; named-pipe-worker, tilakone `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 build-aikana injektoitavalla 32-tavun jaetulla salaisuudella, uudelleenyhdistämisen backoff, jäsennelty observability, salaisuudet redaktoiva log sink.
- **`src/server/connection.rs`** &ndash; hyväksyntäportti: ennen saapuvan vertaisen hyväksymistä konsultoidaan sillan ylläpitämää koneautentikoinnin peer-joukkoa.
- **`src/auth_2fa.rs`** &ndash; 2FA pakotetaan OFF, kun silta hallitsee autentikointia (varmistaa `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay joka näyttää sillan tilan (`active / starting / lost`).
- **`libs/build_support/`** &ndash; aputraite jota `build.rs` ja CI jakavat: tiukka esiehtoportti, hyväksyvä `secret.sec`-parseri, johdonmukaisuustesti protokolladokumentin kanssa.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; wire-protokollan viite.
- **`scripts/check_bridge_strings.ps1`** &ndash; build-jälkeinen vuotoskanneri: takaa, että `HBBS Key` / `VHDMount Key` -tavuja ei vuoda selväkielisinä artefakteihin.
- **`.github/workflows/build.yml`** &mdash; alustojen välinen CI-työnkulku; keskeiset Windows-työt ovat **controller-windows** (Flutter desktop -nippu, oletusominaisuudet + `hwcodec` + `vram` + `flutter`, ei siltaa) ja **controlled-windows** (controlled sidecar, `--features vhd-bridge,controlled-only,hwcodec,vram`), ja se ajaa myös vuoto- ja smoke-skriptit.

Täysi spesifikaatio: [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Kloonaus

Haarukka muuttaa `libs/hbb_common`-alimoduulin URL:n; kloonaa rekursiivisesti:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Jos olet kloonannut aiemmin yläjuoksun `.gitmodules`-tiedostolla: `git submodule sync && git submodule update --init --recursive`.

## Kääntäminen

### Yläjuoksun build (ei siltaa)

Ilman aktiivisia featureita haarukka on yläjuoksun tiukka ylijoukko; yläjuoksun ohjeet pätevät sellaisinaan. Täydet riippuvuudet ja komennot: [`../README.md`](../README.md).

### Build siltakanssa (Windows MSVC, suositellaan)

Silta tukee tällä hetkellä vain Windowsia (named-pipe-kuljetus ja VHDMount-agentti).

Vaadittavat muuttujat:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <polku LLVM\x64\bin>
```

Täytä sitten dev-only `secret.sec` tai aseta vastaavat env-muuttujat, sitten:

```sh
# Tuotanto-sidecar-build (silta päällä, controller poistettu)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# Vain silta (controllerin UI säilyy dev-iteraatioon)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verifiointi

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Viimeisin ajo tällä haaralla: 0 virhettä / 189 unit / 6 + 8 integraatio / 38 + 4 build_support.

## Salaisuudet ja CI

Silta vaatii viisi build-aikaista syötettä:

| Muuttuja | Tarkoitus | Muoto |
|---|---|---|
| `HBBS_KEY` | Rendezvous-palvelimen julkinen avain (korvaa `RS_PUB_KEY`) | base64, 32 tavua dekoodauksen jälkeen |
| `HBBS_HOST` | Rendezvous-palvelimen host | `host[:port[-port2]]` |
| `HBBR_HOST` | Relay-palvelimen host | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (tai `_B64`) | 32-tavuinen jaettu HMAC-salaisuus | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monotoninen avaimen rotaatiota osoittava versio | ei-negatiivinen kokonaisluku |

Kaksi reittiä:

1. **Paikallinen kehitys** &mdash; täytä `secret.sec` repon juuressa riveillä `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Tiedosto ohitetaan [`.gitignore`](../.gitignore):n kautta.
2. **CI** &mdash; aseta samat nimet GitHub Actionsin repo-salaisuuksiksi; [`.github/workflows/build.yml`](../.github/workflows/build.yml) injektoi ne maskaroidu env-muuttujina. **`secret.sec` ei koskaan materialisoidu runnereilla**.

`secret.sec` ja `vhd_bridge_secret.bin` ovat molemmat `.gitignore`ssa, eikä niitä saa **koskaan committaa**. `scripts/check_bridge_strings.ps1` on rakennuksen jälkeinen turvaverkko.

## Lisenssi ja tekijätiedot

Haarukka jaetaan samalla lisenssillä kuin yläjuoksu: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Täysi teksti: [`../LICENCE`](../LICENCE); haarukka **ei muuta** lisenssiä.

- Kaikki tekijänoikeudet yläjuoksun RustDesk-koodikantaan kuuluvat yläjuoksun tekijöille ja avustajille, ks. <https://github.com/rustdesk/rustdesk>.
- Tämän haarukan tuomat muutokset (featuret `vhd-bridge` / `controlled-only` ja tukikoodi) jaetaan myös AGPL-3.0:n alla; alajuoksun käyttäjät säilyttävät kaikki AGPL-3.0:n myöntämät oikeudet, mukaan lukien oikeuden vastaavaan lähdekoodiin verkkoasennuksissa.
- Nimi ja logo "RustDesk" kuuluvat yläjuoksun projektille; haarukka käyttää niitä yksinomaan muunneltua koodikantaa identifioidakseen, vapaiden ohjelmistojen forkkien tavaramerkkien reilun käytön mukaisesti.
- Kolmannen osapuolen kirjastot (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter-riippuvuudet) säilyttävät alkuperäiset lisenssinsä.

Haarukan käyttö merkitsee AGPL-3.0:n ehtojen ja tiedoston alussa olevan **väärinkäyttöä koskevan vastuuvapauslausekkeen** hyväksymistä.
