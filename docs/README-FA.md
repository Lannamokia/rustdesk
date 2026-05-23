<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; فورک <code>Lannamokia</code> با پل احراز هویت ماشین VHD</b><br>
  <a href="#وضعیت-فورک">وضعیت فورک</a> &bull;
  <a href="#این-فورک-چه-چیزی-اضافه-می‌کند">افزوده‌ها</a> &bull;
  <a href="#کامپایل">کامپایل</a> &bull;
  <a href="#اسرار-و-ci">اسرار &amp; CI</a> &bull;
  <a href="#مجوز-و-انتساب">مجوز</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> این مخزن یک فورک پایین‌دستی از [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) است. مستندات کامل انگلیسی: [`../README.md`](../README.md).
> حق تکثیر، نشان‌های تجاری و مجوز AGPL-3.0 بالادست بدون تغییر باقی می‌ماند &mdash; به [مجوز و انتساب](#مجوز-و-انتساب) مراجعه کنید.

> [!Caution]
> **سلب مسئولیت در برابر سوءاستفاده:** توسعه‌دهندگان بالادست RustDesk و نگهدارندگان این فورک هیچ‌گونه استفاده غیراخلاقی یا غیرقانونی از این نرم‌افزار را تأیید یا پشتیبانی نمی‌کنند. دسترسی، کنترل یا تجاوز به حریم خصوصی بدون اجازه اکیداً ممنوع است. نویسندگان مسئولیتی در قبال سوءاستفاده ندارند.

---

## وضعیت فورک

| | |
|---|---|
| **بالادست** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (در git به‌عنوان remote `upstream`) |
| **این فورک** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **شاخه فعال** | `feature/vhd-machine-auth-bridge` |
| **زیرماژول** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common)، همان شاخه |
| **مجوز** | AGPL-3.0 (بدون تغییر نسبت به بالادست &mdash; به [`LICENCE`](../LICENCE) مراجعه کنید) |
| **هدف** | اجرای سمت کنترل‌شونده RustDesk به‌صورت sidecar برای عامل خارجی VHDMount از طریق پلی احراز هویت‌شده و گره‌خورده به ماشین. |

وقتی `vhd-bridge` **خاموش** است، اَرتیفکت ساخت رفتاراً معادل RustDesk بالادست است &mdash; این عدم تغییر را `tests/feature_off_parity.rs` به‌طور خودکار اعتبارسنجی می‌کند.

## این فورک چه چیزی اضافه می‌کند

یک زیرسیستم منسجم &mdash; **پل احراز هویت ماشین VHD** &mdash; که با دو ویژگی Cargo کنترل می‌شود و **به‌طور پیش‌فرض خاموش** هستند:

- **`vhd-bridge`** &mdash; کارگر پل، اتصال IPC، رویه نگه‌داری در UI و آزمون‌های smoke را در ساخت گنجانده می‌کند.
- **`controlled-only`** &mdash; UI و مسیرهای کد Controller (initiator) را حذف می‌کند تا یک باینری فقط-قابل-کنترل تولید کند؛ همراه با `vhd-bridge` برای ساخت sidecar تولیدی استفاده می‌شود.

بدون فعال بودن هیچ ویژگی، `cargo run` و جریان ساخت بالادست بدون تغییر کار می‌کنند.

### تغییرات اصلی

- **`src/vhd_bridge/`** &ndash; کارگر named pipe، ماشین حالت `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`، HMAC-SHA256 با راز مشترک ۳۲ بایتی تزریق‌شده در زمان ساخت، عقب‌نشینی اتصال مجدد، رصدپذیری ساختاریافته، log sink با حذف اسرار.
- **`src/server/connection.rs`** &ndash; دروازه تأیید: پیش از پذیرش peer ورودی، مجموعه peer احراز هویت ماشین که پل نگه می‌دارد بررسی می‌شود.
- **`src/auth_2fa.rs`** &ndash; 2FA تا زمانی که پل بر احراز هویت حاکم است به‌اجبار خاموش می‌شود (`tests/smoke_2fa_disabled.rs` تأیید می‌کند).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay که وضعیت پل (`active / starting / lost`) را نمایش می‌دهد.
- **`libs/build_support/`** &ndash; crate کمکی مشترک بین `build.rs` و CI: دروازه پیش‌نیاز سخت‌گیر، parser تساهل‌گر برای `secret.sec`، آزمون انسجام در برابر سند پروتکل.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; مرجع پروتکل سیمی.
- **`scripts/check_bridge_strings.ps1`** &ndash; اسکنر نشت پس از ساخت: تضمین می‌کند بایت‌های پلِین‌تکست `HBBS Key` / `VHDMount Key` به آرتیفکت‌ها نشت نکنند.
- **`.github/workflows/vhd-bridge.yml`** &mdash; ماتریس CI که آرتیفکت‌های Windows را برای feature-on / feature-off / controlled-only می‌سازد.

مشخصات کامل در [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## کلون

فورک URL زیرماژول `libs/hbb_common` را تغییر می‌دهد؛ به‌صورت بازگشتی کلون کنید:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

اگر قبلاً با `.gitmodules` بالادست کلون کرده‌اید: `git submodule sync && git submodule update --init --recursive`.

## کامپایل

### ساخت بالادست (بدون پل)

بدون فعال بودن ویژگی‌ها، فورک ابرمجموعه‌ای دقیق از بالادست است؛ دستورات بالادست بدون تغییر اعمال می‌شوند. وابستگی‌ها و فرامین کامل: [`../README.md`](../README.md).

### ساخت با پل (Windows MSVC، توصیه‌شده)

پل اکنون فقط Windows را پشتیبانی می‌کند (انتقال named pipe و عامل VHDMount).

محیط لازم:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <مسیر LLVM\x64\bin>
```

سپس `secret.sec` (مخصوص توسعه) را پر کنید یا متغیرهای محیطی متناظر را تنظیم کنید، سپس:

```sh
# ساخت sidecar تولیدی (پل روشن، کنترلر حذف‌شده)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# فقط پل (UI کنترلر برای توسعه حفظ می‌شود)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### اعتبارسنجی

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

آخرین اجرا روی این شاخه: ۰ خطا / ۱۸۹ unit / ۶ + ۸ یکپارچه‌سازی / ۳۸ + ۴ build_support.

## اسرار و CI

پل پنج ورودی زمان ساخت می‌خواهد:

| متغیر | کاربرد | قالب |
|---|---|---|
| `HBBS_KEY` | کلید عمومی سرور rendezvous (`RS_PUB_KEY` را بازنویسی می‌کند) | base64، پس از رمزگشایی ۳۲ بایت |
| `HBBS_HOST` | میزبان سرور rendezvous | `host[:port[-port2]]` |
| `HBBR_HOST` | میزبان سرور relay | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (یا `_B64`) | راز مشترک HMAC به طول ۳۲ بایت | ۶۴ hex / ۴۴ base64 |
| `VHD_BRIDGE_SECRET_VERSION` | نسخه چرخش کلید یکنوای صعودی | عدد صحیح نامنفی |

دو مسیر:

1. **توسعه محلی** &mdash; `secret.sec` را در ریشه مخزن با خطوط `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:` پر کنید. این فایل توسط [`.gitignore`](../.gitignore) نادیده گرفته می‌شود.
2. **CI** &mdash; همان نام‌ها را به‌عنوان repository secret در GitHub Actions تنظیم کنید؛ [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) آن‌ها را به‌صورت متغیرهای محیطی ماسک‌شده تزریق می‌کند. **`secret.sec` هرگز روی runner مادیت نمی‌یابد**.

`secret.sec` و `vhd_bridge_secret.bin` هر دو در `.gitignore` هستند و **هرگز نباید commit شوند**. `scripts/check_bridge_strings.ps1` تور ایمنی پس از ساخت است.

## مجوز و انتساب

این فورک تحت همان مجوز بالادست منتشر می‌شود: **GNU Affero General Public License v3.0 (AGPL-3.0)**. متن کامل در [`../LICENCE`](../LICENCE)؛ فورک **مجوز را تغییر نمی‌دهد**.

- تمامی حق تکثیر کد بالادست RustDesk نزد نویسندگان و مشارکت‌کنندگان بالادست باقی می‌ماند، رجوع کنید به <https://github.com/rustdesk/rustdesk>.
- تغییرات این فورک (ویژگی‌های `vhd-bridge` / `controlled-only` و کد پشتیبان) نیز تحت AGPL-3.0 توزیع می‌شود؛ کاربران پایین‌دستی همه حقوقی را که AGPL-3.0 اعطا می‌کند، از جمله حق دریافت کد منبع متناظر برای هر استقرار شبکه، حفظ می‌کنند.
- نام و لوگوی "RustDesk" متعلق به پروژه بالادست هستند؛ این فورک آن‌ها را صرفاً برای شناسایی پایه کد اصلاح‌شده استفاده می‌کند، در چارچوب استفاده منصفانه از علائم تجاری در فورک‌های نرم‌افزار آزاد.
- کتابخانه‌های شخص ثالث (vcpkg: `libvpx`، `libyuv`، `opus`، `aom`؛ Sciter SDK؛ وابستگی‌های Flutter) مجوزهای اصلی خود را حفظ می‌کنند.

استفاده از این فورک به منزله پذیرش AGPL-3.0 و **سلب مسئولیت در برابر سوءاستفاده** در ابتدای فایل است.
