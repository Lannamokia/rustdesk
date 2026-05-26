<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> cu puntea de autentificare a mașinii VHD</b><br>
  <a href="#starea-fork-ului">Starea fork-ului</a> &bull;
  <a href="#ce-adaugă-acest-fork">Adăugiri</a> &bull;
  <a href="#compilare">Compilare</a> &bull;
  <a href="#secrete-și-ci">Secrete &amp; CI</a> &bull;
  <a href="#licență-și-atribuire">Licență</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>]
</p>

> [!Important]
> Acest depozit este un fork derivat al [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Documentația engleză completă: [`../README.md`](../README.md).
> Drepturile de autor, mărcile și licența AGPL-3.0 ale upstream-ului rămân nemodificate &mdash; vezi [Licență și atribuire](#licență-și-atribuire).

> [!Caution]
> **Renunțare privind utilizarea abuzivă:** dezvoltatorii upstream RustDesk și mentenanții acestui fork nu acceptă și nu sprijină utilizarea neetică sau ilegală a acestui software. Accesul, controlul sau invadarea intimității neautorizate sunt strict interzise. Autorii nu răspund pentru utilizare abuzivă.

---

## Starea fork-ului

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (în git ca remote `upstream`) |
| **Acest fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Ramura activă** | `feature/vhd-machine-auth-bridge` |
| **Submodul** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), aceeași ramură |
| **Licență** | AGPL-3.0 (neschimbată față de upstream &mdash; vezi [`LICENCE`](../LICENCE)) |
| **Scop** | Rularea părții controlate a RustDesk ca sidecar al agentului extern VHDMount, peste o punte autentificată legată de mașină. |

Când `vhd-bridge` este **dezactivat**, artefactul construit este echivalent comportamental cu RustDesk upstream &mdash; invariantul este verificat automat de `tests/feature_off_parity.rs`.

## Ce adaugă acest fork

Un subsistem coerent &mdash; **puntea de autentificare a mașinii VHD** &mdash; controlat de două feature-uri Cargo, **dezactivate implicit**:

- **`vhd-bridge`** &mdash; compilează worker-ul punții, cablajul IPC, overlay-ul UI de mentenanță și testele smoke.
- **`controlled-only`** &mdash; elimină UI-ul și căile de cod ale Controllerului (initiator), producând un binar numai-controlat; combinat cu `vhd-bridge` pentru build-ul sidecar de producție.

Fără feature-uri active, `cargo run` și fluxul de build upstream rulează identic.

### Schimbări principale

- **`src/vhd_bridge/`** &ndash; worker named pipe, automat `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 cu secret comun de 32 octeți injectat la build, backoff de reconectare, observabilitate structurată, log sink care redactează secretele.
- **`src/server/connection.rs`** &ndash; poartă de aprobare: înainte de a accepta peer-ul intrant, consultă peer-set-ul de autentificare a mașinii întreținut de punte.
- **`src/auth_2fa.rs`** &ndash; 2FA forțat OFF cât timp puntea guvernează autentificarea (verifică `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay reflectând starea punții (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate auxiliar partajat între `build.rs` și CI: poartă strictă de prerechizite, parser tolerant pentru `secret.sec`, test de coerență cu documentul de protocol.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; referință protocol de fir.
- **`scripts/check_bridge_strings.ps1`** &ndash; scanner post-build de scurgeri: garantează că niciun octet în clar de `HBBS Key` / `VHDMount Key` nu intră în artefacte.
- **`.github/workflows/build.yml`** &mdash; flux de lucru CI multi-platformă; job-urile Windows-cheie sunt **controller-windows** (bundle Flutter desktop, features implicite + `hwcodec` + `vram` + `flutter`, fără bridge) și **controlled-windows** (sidecar controlled, `--features vhd-bridge,controlled-only,hwcodec,vram`); rulează și scripturile de scurgere + smoke.

Specificația completă în [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Clonare

Fork-ul modifică URL-ul submodulului `libs/hbb_common`; clonează recursiv:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Dacă ai clonat anterior cu `.gitmodules` upstream: `git submodule sync && git submodule update --init --recursive`.

## Compilare

### Build upstream (fără punte)

Fără feature-uri active, fork-ul este o supermulțime strictă a upstream-ului; instrucțiunile upstream se aplică nemodificate. Dependențele și comenzile complete în [`../README.md`](../README.md).

### Build cu punte (Windows MSVC, recomandat)

Puntea suportă în prezent doar Windows (transport named pipe și agent VHDMount).

Mediul cerut:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <calea către LLVM\x64\bin>
```

Apoi populează `secret.sec` (doar pentru dev) sau setează variabilele de mediu corespunzătoare, apoi:

```sh
# Build sidecar de producție (punte ON, controller eliminat)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# Doar punte (UI controller păstrat pentru iterații dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verificare

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Ultima rulare pe această ramură: 0 erori / 189 unit / 6 + 8 integrare / 38 + 4 build_support.

## Secrete și CI

Puntea cere cinci intrări la build:

| Variabilă | Scop | Format |
|---|---|---|
| `HBBS_KEY` | Cheie publică rendezvous server (suprascrie `RS_PUB_KEY`) | base64, 32 octeți decodați |
| `HBBS_HOST` | Host rendezvous server | `host[:port[-port2]]` |
| `HBBR_HOST` | Host relay server | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (sau `_B64`) | Secret comun HMAC de 32 octeți | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Versiune de rotație monotonă | întreg ne-negativ |

Două căi:

1. **Dev local** &mdash; populează `secret.sec` în rădăcina depozitului cu `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Fișierul este ignorat de [`.gitignore`](../.gitignore).
2. **CI** &mdash; configurează aceleași nume ca repository secrets în GitHub Actions; [`.github/workflows/build.yml`](../.github/workflows/build.yml) le injectează ca variabile de mediu mascate. **`secret.sec` nu este materializat niciodată pe runners**.

`secret.sec` și `vhd_bridge_secret.bin` sunt amândouă în `.gitignore` și **nu trebuie commitate niciodată**. `scripts/check_bridge_strings.ps1` este plasa de siguranță post-build.

## Licență și atribuire

Fork-ul se distribuie sub aceeași licență ca upstream-ul: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Text complet în [`../LICENCE`](../LICENCE); fork-ul **nu modifică** licența.

- Toate drepturile de autor asupra codului sursă RustDesk upstream rămân la autorii și contribuitorii upstream, vezi <https://github.com/rustdesk/rustdesk>.
- Modificările introduse de acest fork (feature-urile `vhd-bridge` / `controlled-only` și codul auxiliar) sunt distribuite tot sub AGPL-3.0; utilizatorii downstream păstrează toate drepturile acordate de AGPL-3.0, inclusiv dreptul la codul sursă corespunzător pentru orice implementare în rețea.
- Numele și logo-ul "RustDesk" aparțin proiectului upstream; fork-ul le folosește exclusiv pentru identificarea bazei de cod modificate, în acord cu uzul corect al mărcilor în fork-uri de software liber.
- Bibliotecile terțe (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; SDK Sciter; dependențe Flutter) își păstrează licențele originale.

Folosirea acestui fork înseamnă acceptarea AGPL-3.0 și a **renunțării privind utilizarea abuzivă** de la începutul fișierului.
