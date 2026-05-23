<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> z mostem uwierzytelniania maszyny VHD</b><br>
  <a href="#status-forka">Status forka</a> &bull;
  <a href="#co-dodaje-ten-fork">Dodatki</a> &bull;
  <a href="#kompilacja">Kompilacja</a> &bull;
  <a href="#sekrety-i-ci">Sekrety &amp; CI</a> &bull;
  <a href="#licencja-i-przypisanie">Licencja</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> To repozytorium jest forkiem [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Pełna dokumentacja angielska: [`../README.md`](../README.md).
> Prawa autorskie, znaki towarowe i licencja AGPL-3.0 upstream pozostają niezmienione &mdash; zob. [Licencja i przypisanie](#licencja-i-przypisanie).

> [!Caution]
> **Zastrzeżenie dot. nadużyć:** twórcy upstream RustDesk i opiekunowie tego forka nie tolerują ani nie wspierają jakiegokolwiek nieetycznego ani nielegalnego użycia tego oprogramowania. Nieautoryzowany dostęp, kontrola lub naruszanie prywatności są surowo zabronione. Autorzy nie ponoszą odpowiedzialności za nadużycia.

---

## Status forka

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (w git skonfigurowany jako remote `upstream`) |
| **Ten fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktywna gałąź** | `feature/vhd-machine-auth-bridge` |
| **Submoduł** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), ta sama gałąź |
| **Licencja** | AGPL-3.0 (bez zmian względem upstream &mdash; zob. [`LICENCE`](../LICENCE)) |
| **Cel** | Strona Controlled RustDesk działa jako sidecar zewnętrznego agenta VHDMount przez uwierzytelniony, przypięty do maszyny most. |

Gdy `vhd-bridge` jest **wyłączony**, artefakt buildu jest behawioralnie równoważny upstream-RustDesk &mdash; ten niezmiennik jest weryfikowany przez `tests/feature_off_parity.rs`.

## Co dodaje ten fork

Spójny podsystem &mdash; **most uwierzytelniania maszyny VHD** &mdash; sterowany dwoma feature'ami Cargo, **wyłączonymi domyślnie**:

- **`vhd-bridge`** &mdash; wkompilowuje worker mostu, oprzewodowanie IPC, UI overlay konserwacyjny i testy smoke.
- **`controlled-only`** &mdash; usuwa UI i ścieżki kodu Controllera (initiator), zostawiając binarkę tylko-kontrolowaną; łączyć z `vhd-bridge` dla buildu sidecar produkcyjnego.

Bez aktywnych feature'ów `cargo run` i przepływ buildu upstream działają identycznie.

### Główne zmiany

- **`src/vhd_bridge/`** &ndash; worker named pipe, automat stanów `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 z 32-bajtowym sekretem wstrzykiwanym przy buildzie, backoff reconnectów, ustrukturyzowana obserwowalność, log sink redigujący sekrety.
- **`src/server/connection.rs`** &ndash; bramka aprobaty: przed zaakceptowaniem peera konsultuje machine-auth peer set utrzymywany przez most.
- **`src/auth_2fa.rs`** &ndash; 2FA wymuszone OFF, gdy most rządzi uwierzytelnianiem (weryfikuje `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay odzwierciedlający stan mostu (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate pomocnicza dzielona przez `build.rs` i CI: ścisła bramka prerequisitów, tolerancyjny parser `secret.sec`, test spójności z dokumentacją protokołu.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; referencja protokołu wire.
- **`scripts/check_bridge_strings.ps1`** &ndash; post-build skaner wycieków: zapewnia, że jawnotekstowe bajty `HBBS Key` / `VHDMount Key` nie wyciekają do artefaktów.
- **`.github/workflows/vhd-bridge.yml`** &mdash; macierz CI budująca artefakty Windows feature-on / feature-off / controlled-only.

Pełna specyfikacja: [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klonowanie

Fork zmienia URL submodułu `libs/hbb_common`, więc klonuj rekurencyjnie:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Jeśli klonowano z upstream-owym `.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Kompilacja

### Build upstream (bez mostu)

Bez aktywnych feature'ów fork jest ścisłym nadzbiorem upstream; instrukcje upstream stosują się bez zmian. Pełne zależności i komendy w [`../README.md`](../README.md).

### Build z mostem (Windows MSVC, zalecany)

Most aktualnie wspiera tylko Windows (transport named pipe i agent VHDMount).

Wymagane środowisko:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <ścieżka do LLVM\x64\bin>
```

Następnie wypełnij dev-only `secret.sec` lub ustaw odpowiednie zmienne środowiska, potem:

```sh
# Build sidecar produkcyjny (most ON, controller usunięty)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Sam most (UI controllera zachowane na potrzeby dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Weryfikacja

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Ostatni przebieg na tej gałęzi: 0 błędów / 189 unit / 6 + 8 integracyjnych / 38 + 4 build_support.

## Sekrety i CI

Most wymaga pięciu wejść build-time:

| Zmienna | Cel | Format |
|---|---|---|
| `HBBS_KEY` | Klucz publiczny rendezvous serwera (nadpisuje `RS_PUB_KEY`) | base64, 32 bajty po dekodowaniu |
| `HBBS_HOST` | Host rendezvous | `host[:port[-port2]]` |
| `HBBR_HOST` | Host relay | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (lub `_B64`) | 32-bajtowy współdzielony sekret HMAC | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monotoniczna wersja rotacji klucza | nieujemna liczba całkowita |

Dwie ścieżki:

1. **Lokalna deweloperska** &mdash; wypełnij `secret.sec` w katalogu głównym z liniami `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Plik ignorowany przez [`.gitignore`](../.gitignore).
2. **CI** &mdash; ustaw te same nazwy jako repository secrets w GitHub Actions; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) wstrzykuje je jako maskowane zmienne. **`secret.sec` nigdy nie ląduje na runnerach**.

`secret.sec` i `vhd_bridge_secret.bin` są w `.gitignore` i **nigdy** nie wolno ich commitować. `scripts/check_bridge_strings.ps1` to siatka bezpieczeństwa po buildzie.

## Licencja i przypisanie

Ten fork jest dystrybuowany na tej samej licencji co upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Pełen tekst w [`../LICENCE`](../LICENCE); fork **nie modyfikuje** licencji.

- Wszelkie prawa autorskie do bazy kodu RustDesk upstream pozostają przy autorach i kontrybutorach upstream, zob. <https://github.com/rustdesk/rustdesk>.
- Modyfikacje wprowadzone przez ten fork (feature'y `vhd-bridge` / `controlled-only` i kod wspierający) są również dystrybuowane na AGPL-3.0; użytkownicy downstream zachowują wszystkie prawa nadane przez AGPL-3.0, w tym prawo do otrzymania odpowiadającego kodu źródłowego dla każdego wdrożenia sieciowego.
- Nazwa i logo "RustDesk" są własnością projektu upstream; fork używa ich wyłącznie do identyfikacji modyfikowanej bazy kodu, w ramach uczciwego użycia znaków towarowych w forkach wolnego oprogramowania.
- Biblioteki zewnętrzne (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; zależności Flutter) zachowują swoje oryginalne licencje.

Korzystanie z tego forka oznacza akceptację AGPL-3.0 i **zastrzeżenia dot. nadużyć** na początku pliku.
