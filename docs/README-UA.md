<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; форк <code>Lannamokia</code> з мостом машинної автентифікації VHD</b><br>
  <a href="#стан-форку">Стан форку</a> &bull;
  <a href="#що-додає-цей-форк">Зміни</a> &bull;
  <a href="#збірка">Збірка</a> &bull;
  <a href="#секрети-та-ci">Секрети &amp; CI</a> &bull;
  <a href="#ліцензія-та-атрибуція">Ліцензія</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Цей репозиторій є downstream-форком [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Повна англомовна документація: [`../README.md`](../README.md).
> Авторські права upstream, торговельні марки та ліцензія AGPL-3.0 не змінено &mdash; див. [Ліцензія та атрибуція](#ліцензія-та-атрибуція).

> [!Caution]
> **Застереження щодо зловживань:** розробники upstream-RustDesk та супровідники цього форка не схвалюють і не підтримують жодного неетичного чи незаконного використання цього ПЗ. Несанкціонований доступ, керування або порушення приватності суворо заборонені. Автори не несуть відповідальності за будь-яке зловживання застосунком.

---

## Стан форку

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (у git це remote `upstream`) |
| **Цей форк** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Активна гілка** | `feature/vhd-machine-auth-bridge` |
| **Субмодуль** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), та сама гілка |
| **Ліцензія** | AGPL-3.0 (без змін відносно upstream &mdash; див. [`LICENCE`](../LICENCE)) |
| **Мета** | Запускати керовану сторону RustDesk як sidecar зовнішнього агента VHDMount через автентифікований міст, прив'язаний до машини. |

Коли `vhd-bridge` **вимкнено**, артефакт збірки поведінково ідентичний upstream-RustDesk &mdash; інваріант перевіряється `tests/feature_off_parity.rs`.

## Що додає цей форк

Зв'язну підсистему &mdash; **міст машинної автентифікації VHD** &mdash; керовану двома Cargo-фічами, **за замовчуванням вимкненими**:

- **`vhd-bridge`** &mdash; вкомпільовує воркер моста, IPC-обв'язку, UI-оверлей обслуговування та smoke-тести.
- **`controlled-only`** &mdash; видаляє Controller (initiator) UI та шляхи коду, лишаючи лише-керовану бінарь; поєднується з `vhd-bridge` для бойової sidecar-збірки.

Без активних фіч `cargo run` та upstream-збірка працюють без змін.

### Основні зміни

- **`src/vhd_bridge/`** &ndash; named-pipe-воркер, скінченний автомат `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 на 32-байтовому спільному секреті, що інжектується під час збірки, backoff перепідключень, структурована спостережуваність, log sink з редагуванням секретів.
- **`src/server/connection.rs`** &ndash; шлюз підтвердження: перед прийняттям вхідного peer-а звертається до набору machine-auth-peer-ів, який підтримує міст.
- **`src/auth_2fa.rs`** &ndash; 2FA примусово вимкнено, поки міст керує автентифікацією (перевіряє `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; UI-оверлей зі станом моста (`active / starting / lost`).
- **`libs/build_support/`** &ndash; допоміжна crate для `build.rs` та CI: суворий шлюз передумов, толерантний парсер `secret.sec`, тест узгодженості з документом протоколу.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; референс протоколу.
- **`scripts/check_bridge_strings.ps1`** &ndash; пост-збірковий сканер витоків: гарантує, що відкриті байти `HBBS Key` / `VHDMount Key` не потраплять у артефакти.
- **`.github/workflows/vhd-bridge.yml`** &mdash; CI-матриця, що збирає Windows-артефакти feature-on / feature-off / controlled-only.

Повна специфікація: [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Клонування

Форк змінює URL субмодуля `libs/hbb_common`, тож клонуйте рекурсивно:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Якщо вже клонували з upstream-овим `.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Збірка

### Upstream-збірка (без моста)

Без активних фіч цей форк є строгою надмножиною upstream; інструкції upstream чинні без змін. Повні залежності та команди див. у [`../README.md`](../README.md).

### Збірка з мостом (Windows MSVC, рекомендовано)

Міст наразі підтримує лише Windows (named-pipe-транспорт та VHDMount-агент).

Необхідні змінні середовища:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <шлях до LLVM\x64\bin>
```

Далі заповніть dev-only `secret.sec` або задайте відповідні env-змінні, тоді:

```sh
# Бойова sidecar-збірка (міст увімкнено, контролер прибрано)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Лише міст (UI-контролера лишається для розробки)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Верифікація

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Останній запуск у цій гілці: 0 помилок / 189 unit / 6 + 8 інтеграційних / 38 + 4 build_support.

## Секрети та CI

Міст вимагає п'ять входів на час збірки:

| Змінна | Призначення | Формат |
|---|---|---|
| `HBBS_KEY` | Публічний ключ rendezvous-сервера (перекриває `RS_PUB_KEY`) | base64, після декодування 32 байти |
| `HBBS_HOST` | Хост rendezvous-сервера | `host[:port[-port2]]` |
| `HBBR_HOST` | Хост relay-сервера | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (або `_B64`) | 32-байтовий спільний HMAC-секрет | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Монотонна версія ротації ключа | невід'ємне ціле |

Два шляхи:

1. **Локальна розробка** &mdash; заповніть `secret.sec` у корені репозиторію рядками `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Файл ігнорується через [`.gitignore`](../.gitignore).
2. **CI** &mdash; ті ж імена як repository-secrets у GitHub Actions; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) інжектує їх як замасковані env-змінні. **`secret.sec` ніколи не матеріалізується на runner-ах**.

`secret.sec` і `vhd_bridge_secret.bin` обидва в `.gitignore` і **ніколи** не повинні комітитися. `scripts/check_bridge_strings.ps1` &mdash; страхувальна сітка після збірки.

## Ліцензія та атрибуція

Цей форк поширюється під тією ж ліцензією, що й upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Повний текст у [`../LICENCE`](../LICENCE); цей форк ліцензію **не змінює**.

- Усі авторські права на upstream-кодову базу RustDesk належать upstream-авторам і контриб'юторам, див. <https://github.com/rustdesk/rustdesk>.
- Зміни цього форка (фічі `vhd-bridge` / `controlled-only` та супровідний код) також поширюються під AGPL-3.0; downstream-користувачі зберігають усі права, надані AGPL-3.0, зокрема право на отримання відповідного джерельного коду під час будь-якого мережевого розгортання.
- Назва та логотип "RustDesk" належать upstream-проєкту; форк використовує їх лише для ідентифікації модифікованої кодової бази в межах добросовісного використання торговельних марок у форках вільного ПЗ.
- Сторонні бібліотеки (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; залежності Flutter) зберігають свої оригінальні ліцензії.

Користуючись форком, ви погоджуєтеся з умовами AGPL-3.0 та **застереженням щодо зловживань** на початку файлу.
