<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; VHD മെഷീൻ-ഓത്ത് ബ്രിഡ്ജ് ഉൾപ്പെട്ട <code>Lannamokia</code> ഫോർക്ക്</b><br>
  <a href="#ഫോർക്ക്-സ്റ്റാറ്റസ്">ഫോർക്ക് സ്റ്റാറ്റസ്</a> &bull;
  <a href="#ഈ-ഫോർക്ക്-എന്ത്-ചേർക്കുന്നു">കൂട്ടിച്ചേർപ്പുകൾ</a> &bull;
  <a href="#ബിൽഡ്">ബിൽഡ്</a> &bull;
  <a href="#രഹസ്യങ്ങളും-ci-യും">രഹസ്യങ്ങളും CI</a> &bull;
  <a href="#ലൈസൻസും-ആട്രിബ്യൂഷനും">ലൈസൻസ്</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> ഈ റിപ്പോസിറ്ററി [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) എന്നതിന്റെ ഡൗൺസ്ട്രീം ഫോർക്കാണ്. പൂർണ്ണ ഇംഗ്ലീഷ് ഡോക്യുമെന്റേഷൻ: [`../README.md`](../README.md).
> അപ്സ്ട്രീമിന്റെ പകർപ്പവകാശം, ട്രേഡ്മാർക്കുകൾ, AGPL-3.0 ലൈസൻസ് മാറ്റമില്ലാതെ തുടരുന്നു &mdash; [ലൈസൻസും ആട്രിബ്യൂഷനും](#ലൈസൻസും-ആട്രിബ്യൂഷനും) കാണുക.

> [!Caution]
> **ദുരുപയോഗ ഉത്തരവാദിത്വ നിരാകരണം:** അപ്സ്ട്രീം RustDesk ഡെവലപ്പർമാരും ഈ ഫോർക്കിന്റെ പരിപാലകരും ഈ സോഫ്റ്റ്‌വെയറിന്റെ ഏതെങ്കിലും അനീതിയോ നിയമവിരുദ്ധമോ ആയ ഉപയോഗത്തെ പ്രോത്സാഹിപ്പിക്കുന്നില്ല. അനധികൃത പ്രവേശനം, നിയന്ത്രണം, സ്വകാര്യതാ ലംഘനം എന്നിവ കർശനമായി നിരോധിച്ചിരിക്കുന്നു. എഴുത്തുകാർ ദുരുപയോഗത്തിന് ഉത്തരവാദികളല്ല.

---

## ഫോർക്ക് സ്റ്റാറ്റസ്

| | |
|---|---|
| **അപ്സ്ട്രീം** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (git-ൽ `upstream` remote) |
| **ഈ ഫോർക്ക്** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **സജീവ ബ്രാഞ്ച്** | `feature/vhd-machine-auth-bridge` |
| **സബ്മൊഡ്യൂൾ** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), അതേ ബ്രാഞ്ച് |
| **ലൈസൻസ്** | AGPL-3.0 (അപ്സ്ട്രീമിൽ നിന്ന് മാറ്റമില്ല &mdash; [`LICENCE`](../LICENCE) കാണുക) |
| **ലക്ഷ്യം** | ബാഹ്യ VHDMount ഏജന്റിന് വേണ്ടി ഒരു സൈഡ്കാർ ആയി RustDesk-ന്റെ കൺട്രോൾ ചെയ്യപ്പെടുന്ന വശം പ്രവർത്തിപ്പിക്കുക, ഒരു മെഷീനുമായി ബന്ധിപ്പിച്ചതും ഓത്ത് ചെയ്തതുമായ ബ്രിഡ്ജിലൂടെ. |

`vhd-bridge` **നിർജ്ജീവമാക്കിയിരിക്കുമ്പോൾ**, ബിൽഡ് ആർട്ടിഫാക്റ്റ് അപ്സ്ട്രീം RustDesk-ന് സമാനമായി പെരുമാറുന്നു &mdash; `tests/feature_off_parity.rs` ഈ ഇൻവേരിയന്റ് സ്വയം പരിശോധിക്കുന്നു.

## ഈ ഫോർക്ക് എന്ത് ചേർക്കുന്നു

ഒരു ഏകീകൃത ഉപസിസ്റ്റം &mdash; **VHD മെഷീൻ-ഓത്ത് ബ്രിഡ്ജ്** &mdash; രണ്ട് Cargo features-കൾ വഴി നിയന്ത്രിക്കപ്പെടുന്നു, **ഡിഫോൾട്ടായി ഓഫ്**:

- **`vhd-bridge`** &mdash; ബ്രിഡ്ജ് വർക്കർ, IPC വയറിങ്, മെയിന്റനൻസ് overlay UI, smoke ടെസ്റ്റുകൾ കംപൈൽ ചെയ്യുന്നു.
- **`controlled-only`** &mdash; Controller (initiator) UI ഉം കോഡ് പാത്തുകളും നീക്കം ചെയ്ത് കൺട്രോൾ-മാത്രം ബൈനറി ഉണ്ടാക്കുന്നു; പ്രൊഡക്ഷൻ സൈഡ്കാർ ബിൽഡിന് `vhd-bridge`-ഓടൊപ്പം ഉപയോഗിക്കുന്നു.

സജീവ features ഇല്ലെങ്കിൽ, `cargo run`-ഉം അപ്സ്ട്രീം ബിൽഡ് ഫ്ലോയും മാറ്റമില്ലാതെ പ്രവർത്തിക്കും.

### പ്രധാന മാറ്റങ്ങൾ

- **`src/vhd_bridge/`** &ndash; named-pipe വർക്കർ, സ്റ്റേറ്റ് മെഷീൻ `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, ബിൽഡ് സമയത്ത് ഇൻജക്റ്റ് ചെയ്യപ്പെട്ട 32-ബൈറ്റ് പങ്കിട്ട രഹസ്യത്തിൽ HMAC-SHA256, റീകണക്റ്റ് backoff, ഘടനാപരമായ observability, രഹസ്യങ്ങൾ മറയ്ക്കുന്ന log sink.
- **`src/server/connection.rs`** &ndash; അംഗീകാര ഗേറ്റ്: വരുന്ന peer സ്വീകരിക്കുന്നതിന് മുമ്പ് ബ്രിഡ്ജ് നിലനിർത്തുന്ന മെഷീൻ-ഓത്ത് peer-set പരിശോധിക്കുന്നു.
- **`src/auth_2fa.rs`** &ndash; ബ്രിഡ്ജ് ഓത്ത് നിയന്ത്രിക്കുമ്പോൾ 2FA നിർബന്ധിതമായി OFF (`tests/smoke_2fa_disabled.rs` പരിശോധിക്കുന്നു).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; ബ്രിഡ്ജിന്റെ അവസ്ഥ (`active / starting / lost`) പ്രതിഫലിപ്പിക്കുന്ന overlay.
- **`libs/build_support/`** &ndash; `build.rs`-ഉം CI-യും പങ്കിടുന്ന സഹായ crate: കർശന മുൻകൂർ വ്യവസ്ഥ ഗേറ്റ്, സഹിഷ്ണുതയുള്ള `secret.sec` parser, പ്രോട്ടോക്കോൾ ഡോക്യുമെന്റുമായി കൺസിസ്റ്റൻസി പരിശോധന.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; വയർ പ്രോട്ടോക്കോൾ റഫറൻസ്.
- **`scripts/check_bridge_strings.ps1`** &ndash; ബിൽഡിന് ശേഷമുള്ള ലീക്ക് സ്കാനർ: `HBBS Key` / `VHDMount Key`-ന്റെ പ്ലെയിൻ ടെക്സ്റ്റ് ബൈറ്റുകൾ ആർട്ടിഫാക്റ്റുകളിലേക്ക് ലീക്കാകുന്നില്ലെന്ന് ഉറപ്പാക്കുന്നു.
- **`.github/workflows/build.yml`** &mdash; cross-platform CI workflow; the key Windows jobs are **controller-windows** (Flutter desktop bundle, default features + `hwcodec` + `vram` + `flutter`, no bridge) and **controlled-windows** (controlled sidecar, `--features vhd-bridge,controlled-only,hwcodec,vram`); the leakage + smoke scripts run there too.

പൂർണ്ണ വ്യക്തമാക്കൽ [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge)-ൽ.

## ക്ലോൺ

ഫോർക്ക് `libs/hbb_common` സബ്മൊഡ്യൂളിന്റെ URL മാറ്റുന്നതിനാൽ recursive clone ഉപയോഗിക്കുക:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

മുമ്പ് അപ്സ്ട്രീം `.gitmodules`-ൽ ക്ലോൺ ചെയ്തിരുന്നെങ്കിൽ: `git submodule sync && git submodule update --init --recursive`.

## ബിൽഡ്

### അപ്സ്ട്രീം ബിൽഡ് (ബ്രിഡ്ജ് ഇല്ലാതെ)

സജീവ features ഇല്ലാതെ, ഫോർക്ക് അപ്സ്ട്രീമിന്റെ കർശന സൂപ്പർസെറ്റ് ആണ്; അപ്സ്ട്രീം നിർദ്ദേശങ്ങൾ മാറ്റമില്ലാതെ ബാധകം. പൂർണ്ണ ഡിപ്പൻഡൻസികളും കമാൻഡുകളും [`../README.md`](../README.md)-ൽ.

### ബ്രിഡ്ജ് ബിൽഡ് (Windows MSVC, ശുപാർശ ചെയ്യുന്നു)

ബ്രിഡ്ജ് നിലവിൽ Windows-ഉം മാത്രം പിന്തുണയ്ക്കുന്നു (named-pipe transport, VHDMount ഏജന്റ്).

ആവശ്യമായ environment:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <LLVM\x64\bin എന്നതിലേക്കുള്ള പാത>
```

പിന്നെ dev-only `secret.sec` പൂരിപ്പിക്കുക അല്ലെങ്കിൽ അനുബന്ധ env വേരിയബിളുകൾ സജ്ജമാക്കുക, പിന്നെ:

```sh
# പ്രൊഡക്ഷൻ സൈഡ്കാർ ബിൽഡ് (ബ്രിഡ്ജ് ON, controller നീക്കിയിരിക്കുന്നു)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# ബ്രിഡ്ജ് മാത്രം (controller UI dev-ന് സൂക്ഷിച്ചിരിക്കുന്നു)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### പരിശോധന

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

ഈ ബ്രാഞ്ചിൽ അവസാന റൺ: 0 പിശകുകൾ / 189 unit / 6 + 8 integration / 38 + 4 build_support.

## രഹസ്യങ്ങളും CI യും

ബ്രിഡ്ജിന് ബിൽഡ് സമയത്ത് അഞ്ച് ഇൻപുട്ടുകൾ വേണം:

| വേരിയബിൾ | ലക്ഷ്യം | ഫോർമാറ്റ് |
|---|---|---|
| `HBBS_KEY` | rendezvous സെർവർ പബ്ലിക് കീ (`RS_PUB_KEY` overrides ചെയ്യുന്നു) | base64, ഡീകോഡിന് ശേഷം 32 ബൈറ്റുകൾ |
| `HBBS_HOST` | rendezvous സെർവർ host | `host[:port[-port2]]` |
| `HBBR_HOST` | relay സെർവർ host | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (അല്ലെങ്കിൽ `_B64`) | 32-ബൈറ്റ് HMAC പങ്കിട്ട രഹസ്യം | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | മോണോടോണിക് കീ റൊട്ടേഷൻ വേർഷൻ | നോൺ-നെഗറ്റീവ് integer |

രണ്ട് വഴികൾ:

1. **ലോക്കൽ dev** &mdash; റിപ്പോ റൂട്ടിൽ `secret.sec` പൂരിപ്പിക്കുക: `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. ഫയൽ [`.gitignore`](../.gitignore) വഴി അവഗണിക്കപ്പെടുന്നു.
2. **CI** &mdash; അതേ പേരുകൾ GitHub Actions repository secrets ആയി കോൺഫിഗർ ചെയ്യുക; [`.github/workflows/build.yml`](../.github/workflows/build.yml) അവയെ മാസ്ക് ചെയ്ത env വേരിയബിളുകളായി inject ചെയ്യുന്നു. **`secret.sec` runners-ൽ ഒരിക്കലും materialize ചെയ്യില്ല**.

`secret.sec`-ഉം `vhd_bridge_secret.bin`-ഉം `.gitignore`-ലുണ്ട്, അവ **ഒരിക്കലും commit ചെയ്യരുത്**. `scripts/check_bridge_strings.ps1` എന്നത് ബിൽഡിന് ശേഷമുള്ള സുരക്ഷാ വല.

## ലൈസൻസും ആട്രിബ്യൂഷനും

ഈ ഫോർക്ക് അപ്സ്ട്രീമിന്റെ അതേ ലൈസൻസിൽ വിതരണം ചെയ്യപ്പെടുന്നു: **GNU Affero General Public License v3.0 (AGPL-3.0)**. പൂർണ്ണ പാഠം [`../LICENCE`](../LICENCE)-ൽ; ഫോർക്ക് ലൈസൻസ് **മാറ്റുന്നില്ല**.

- അപ്സ്ട്രീം RustDesk കോഡ്ബേസിന്റെ എല്ലാ പകർപ്പവകാശവും അപ്സ്ട്രീം എഴുത്തുകാർക്കും കോൺട്രിബ്യൂട്ടർമാർക്കും ഉണ്ട്, <https://github.com/rustdesk/rustdesk> കാണുക.
- ഈ ഫോർക്ക് കൊണ്ടുവരുന്ന മാറ്റങ്ങൾ (`vhd-bridge` / `controlled-only` features-കളും സഹായ കോഡും) AGPL-3.0-ന് കീഴിലാണ് വിതരണം ചെയ്യുന്നത്; ഡൗൺസ്ട്രീം ഉപയോക്താക്കൾ AGPL-3.0 നൽകുന്ന എല്ലാ അവകാശങ്ങളും, നെറ്റ്‌വർക്ക് ഡിപ്ലോയ്‌മെന്റിലെ അനുബന്ധ source code അവകാശം ഉൾപ്പെടെ, നിലനിർത്തുന്നു.
- "RustDesk" പേരും ലോഗോയും അപ്സ്ട്രീം പ്രോജക്റ്റിന്റേതാണ്; ഫോർക്ക് അവ ഉപയോഗിക്കുന്നത് മാറ്റം വരുത്തിയ കോഡ്ബേസ് തിരിച്ചറിയാൻ മാത്രമാണ്, സ്വതന്ത്ര സോഫ്റ്റ്‌വെയർ ഫോർക്കുകളിൽ ട്രേഡ്മാർക്കുകളുടെ fair use അനുസരിച്ച്.
- തേർഡ്-പാർട്ടി ലൈബ്രറികൾ (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter ഡിപ്പൻഡൻസികൾ) അവയുടെ യഥാർത്ഥ ലൈസൻസുകൾ നിലനിർത്തുന്നു.

ഈ ഫോർക്ക് ഉപയോഗിക്കുന്നത് AGPL-3.0-ഉം ഫയലിന്റെ മുകളിലുള്ള **ദുരുപയോഗ ഉത്തരവാദിത്വ നിരാകരണവും** സ്വീകരിക്കുന്നു എന്ന് സൂചിപ്പിക്കുന്നു.
