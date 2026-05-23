<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; форк <code>Lannamokia</code> с VHD-мостом машинной аутентификации</b><br>
  <a href="#статус-форка">Статус форка</a> &bull;
  <a href="#что-добавляет-этот-форк">Изменения</a> &bull;
  <a href="#сборка">Сборка</a> &bull;
  <a href="#секреты-и-ci">Секреты &amp; CI</a> &bull;
  <a href="#лицензия-и-указание-авторства">Лицензия</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Этот репозиторий &mdash; downstream-форк [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Полная английская документация: [`../README.md`](../README.md).
> Авторские права upstream, торговые знаки и лицензия AGPL-3.0 не изменены &mdash; см. [Лицензия и указание авторства](#лицензия-и-указание-авторства).

> [!Caution]
> **Отказ от ответственности за злоупотребление:** разработчики upstream RustDesk и сопровождающие этого форка не одобряют и не поддерживают неэтичное или незаконное использование данного ПО. Несанкционированный доступ, контроль или нарушение приватности строго запрещены. Авторы не несут ответственности за злоупотребление приложением.

---

## Статус форка

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (в git настроен как remote `upstream`) |
| **Этот форк** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Активная ветка** | `feature/vhd-machine-auth-bridge` |
| **Сабмодуль** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), та же ветка |
| **Лицензия** | AGPL-3.0 (без изменений относительно upstream &mdash; см. [`LICENCE`](../LICENCE)) |
| **Цель** | Запускать управляемую сторону RustDesk как sidecar внешнего агента VHDMount через аутентифицированный мост, привязанный к машине. |

Когда `vhd-bridge` **выключен**, артефакт сборки поведенчески идентичен upstream-RustDesk &mdash; инвариант проверяется автоматически в `tests/feature_off_parity.rs`.

## Что добавляет этот форк

Один цельный подсистема &mdash; **VHD-мост машинной аутентификации** &mdash; управляется двумя Cargo-фичами, **по умолчанию выключенными**:

- **`vhd-bridge`** &mdash; включает воркер моста, IPC-обвязку, UI-оверлей обслуживания и smoke-тесты.
- **`controlled-only`** &mdash; вырезает Controller (initiator) UI и его кодовые пути для получения «только управляемой» бинарника; используется вместе с `vhd-bridge` для боевой sidecar-сборки.

Без активных фич `cargo run` и upstream-сценарии сборки работают идентично оригиналу.

### Основные изменения

- **`src/vhd_bridge/`** &ndash; воркер именованного канала, конечный автомат `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 на 32-байтовом общем секрете, инжектируемом во время сборки, backoff переподключений, структурированная наблюдаемость, log sink с редактированием секретов.
- **`src/server/connection.rs`** &ndash; шлюз одобрения: перед приёмом входящего пира сверяется со списком machine-auth peer-ов, поддерживаемым мостом.
- **`src/auth_2fa.rs`** &ndash; 2FA принудительно выключается, пока мост управляет аутентификацией (проверяется `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; UI-оверлей, отражающий состояние моста (`active / starting / lost`).
- **`libs/build_support/`** &ndash; вспомогательная crate, общая для `build.rs` и CI: строгий шлюз предусловий, толерантный парсер `secret.sec`, тест соответствия документу протокола.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; описание провода-протокола.
- **`scripts/check_bridge_strings.ps1`** &ndash; пост-сборочный сканер утечек: проверяет, что в артефактах нет открытых байт `HBBS Key` / `VHDMount Key`.
- **`.github/workflows/vhd-bridge.yml`** &mdash; CI-матрица сборки feature-on / feature-off / controlled-only Windows-артефактов.

Полная спецификация: [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Клонирование

Форк меняет URL сабмодуля `libs/hbb_common`, поэтому клонируйте рекурсивно:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Если уже клонировали со старым `.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Сборка

### Upstream-сборка (без моста)

Без активных фич форк является строгим надмножеством upstream; инструкции upstream применимы без изменений. Полные зависимости и команды см. в [`../README.md`](../README.md).

### Сборка с мостом (Windows MSVC, рекомендуется)

Мост сейчас поддерживает только Windows (named-pipe-транспорт и агент VHDMount).

Требуемые переменные окружения:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <путь к LLVM\x64\bin>
```

Затем заполните dev-only `secret.sec` или задайте соответствующие переменные окружения и:

```sh
# Боевая sidecar-сборка (мост включён, контроллер удалён)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Только мост (контроллерный UI остаётся для разработки)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Верификация

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Последний прогон на этой ветке: 0 ошибок / 189 unit / 6 + 8 интеграционных / 38 + 4 build_support.

## Секреты и CI

Мост требует пять входов на этапе сборки:

| Переменная | Назначение | Формат |
|---|---|---|
| `HBBS_KEY` | Публичный ключ rendezvous-сервера (перекрывает `RS_PUB_KEY`) | base64, 32 байта после декодирования |
| `HBBS_HOST` | Хост rendezvous-сервера | `host[:port[-port2]]` |
| `HBBR_HOST` | Хост relay-сервера | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (или `_B64`) | 32-байтовый общий секрет HMAC | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Монотонная версия ротации ключа | неотрицательное целое |

Два пути:

1. **Локальная разработка** &mdash; заполнить `secret.sec` в корне репозитория строками `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Файл игнорируется через [`.gitignore`](../.gitignore).
2. **CI** &mdash; настроить одноимённые repository-secrets в GitHub Actions; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) подаёт их как маскированные env-переменные. **`secret.sec` никогда не материализуется на runner-ах**.

`secret.sec` и `vhd_bridge_secret.bin` оба в `.gitignore` и **никогда не должны коммититься**. `scripts/check_bridge_strings.ps1` &mdash; это страховочная сетка после сборки.

## Лицензия и указание авторства

Этот форк распространяется под той же лицензией, что и upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Полный текст: [`../LICENCE`](../LICENCE); форк **не изменяет** его.

- Все авторские права на upstream-кодовую базу RustDesk остаются за её авторами и контрибьюторами, см. <https://github.com/rustdesk/rustdesk>.
- Изменения, вносимые этим форком (фичи `vhd-bridge` / `controlled-only` и сопутствующий код), также распространяются под AGPL-3.0; downstream-пользователи сохраняют все права, предоставляемые AGPL-3.0, в т.ч. право на получение исходников при сетевом развёртывании.
- Имя и логотип "RustDesk" принадлежат upstream-проекту; форк использует их исключительно для идентификации модифицируемой кодовой базы в рамках добросовестного использования товарных знаков для форков свободного ПО.
- Сторонние библиотеки (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; зависимости Flutter) сохраняют свои оригинальные лицензии.

Использование форка означает согласие с условиями AGPL-3.0 и **отказом от ответственности за злоупотребление** в начале файла.
