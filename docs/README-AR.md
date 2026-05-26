<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; نسخة <code>Lannamokia</code> مع جسر مصادقة الجهاز VHD</b><br>
  <a href="#حالة-النسخة">حالة النسخة</a> &bull;
  <a href="#ما-تضيفه-هذه-النسخة">الإضافات</a> &bull;
  <a href="#البناء">البناء</a> &bull;
  <a href="#الأسرار-و-ci">الأسرار &amp; CI</a> &bull;
  <a href="#الترخيص-والإسناد">الترخيص</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> هذا المستودع نسخة فرعية (downstream fork) من [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). الوثائق الإنجليزية الكاملة: [`../README.md`](../README.md).
> حقوق التأليف والعلامات التجارية ورخصة AGPL-3.0 الخاصة بالأصل لم تتغير &mdash; راجع [الترخيص والإسناد](#الترخيص-والإسناد).

> [!Caution]
> **إخلاء مسؤولية الاستخدام السيئ:** لا يتسامح مطورو RustDesk الأصليون ولا مشرفو هذه النسخة مع أي استخدام غير أخلاقي أو غير قانوني لهذا البرنامج. الوصول أو التحكم أو انتهاك الخصوصية دون إذن ممنوع تماماً. لا يتحمل المؤلفون أي مسؤولية عن الاستخدام السيئ.

---

## حالة النسخة

| | |
|---|---|
| **الأصل (upstream)** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (في git مُعرَّف كـ remote `upstream`) |
| **هذه النسخة** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **الفرع النشط** | `feature/vhd-machine-auth-bridge` |
| **الوحدة الفرعية** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common)، نفس الفرع |
| **الترخيص** | AGPL-3.0 (دون تغيير عن الأصل &mdash; راجع [`LICENCE`](../LICENCE)) |
| **الهدف** | تشغيل الجانب المُتحكَّم به من RustDesk بصفته sidecar لوكيل VHDMount الخارجي عبر جسر مُصادَق ومُربط بالجهاز. |

عندما تكون `vhd-bridge` **معطلة**، يكون مخرج البناء مكافئاً سلوكياً لـ RustDesk الأصلي &mdash; يتحقق هذا الثبات تلقائياً عبر `tests/feature_off_parity.rs`.

## ما تضيفه هذه النسخة

نظام فرعي متماسك واحد &mdash; **جسر مصادقة الجهاز VHD** &mdash; تتحكم فيه ميزتان من Cargo، **معطلتان افتراضياً**:

- **`vhd-bridge`** &mdash; تُضمِّن في الترجمة worker الجسر وتمديد IPC وواجهة الصيانة overlay واختبارات smoke.
- **`controlled-only`** &mdash; تُزيل واجهة وتجاويف الكود الخاصة بـ Controller (initiator) لإنتاج ثنائي يمكن التحكم به فقط؛ تُمزج مع `vhd-bridge` لبناء sidecar الإنتاجي.

بدون أي ميزة نشطة يعمل `cargo run` وتدفق بناء الأصل دون تغيير.

### التغييرات الرئيسية

- **`src/vhd_bridge/`** &ndash; worker لـ named pipe، آلة الحالات `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`، HMAC-SHA256 بسر مشترك من 32 بايت يُحقن وقت البناء، backoff لإعادة الاتصال، رصد منظم، log sink يحجب الأسرار.
- **`src/server/connection.rs`** &ndash; بوابة الموافقة: قبل قبول peer وارد، يُستشار set peer مصادقة الجهاز الذي يحافظ عليه الجسر.
- **`src/auth_2fa.rs`** &ndash; يُجبر 2FA على الإيقاف ما دام الجسر يحكم المصادقة (تتحقق `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay يعكس حالة الجسر (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate مساعد يتشاركه `build.rs` و CI: بوابة متطلبات صارمة، parser متسامح لـ `secret.sec`، اختبار اتساق مع وثيقة البروتوكول.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; مرجع بروتوكول السلك.
- **`scripts/check_bridge_strings.ps1`** &ndash; ماسح تسريب ما بعد البناء: يضمن عدم تسرب بايتات `HBBS Key` / `VHDMount Key` بنص واضح إلى المخرجات.
- **`.github/workflows/build.yml`** &mdash; سير عمل CI متعدد المنصات؛ مهمتا Windows الرئيسيتان هما **controller-windows** (حزمة Flutter desktop، features الافتراضية + `hwcodec` + `vram` + `flutter`، بدون bridge) و **controlled-windows** (sidecar مُتحكَّم به، `--features vhd-bridge,controlled-only,hwcodec,vram`)، ويشغّل أيضاً سكريبتات التسريب و smoke.

المواصفة الكاملة في [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## الاستنساخ

تُغيِّر النسخة عنوان وحدة `libs/hbb_common` الفرعية؛ استنسخ بشكل تكراري:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

إن سبق الاستنساخ بـ `.gitmodules` الأصلي: `git submodule sync && git submodule update --init --recursive`.

## البناء

### بناء الأصل (دون جسر)

بدون أي ميزة نشطة، النسخة هي مجموعة فوقية صارمة من الأصل؛ تعليمات الأصل تنطبق دون تغيير. التبعيات والأوامر الكاملة في [`../README.md`](../README.md).

### البناء مع الجسر (Windows MSVC، موصى به)

الجسر حالياً يدعم Windows فقط (نقل named pipe ووكيل VHDMount).

البيئة المطلوبة:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <مسار LLVM\x64\bin>
```

ثم املأ `secret.sec` (للتطوير فقط) أو حدد متغيرات البيئة المقابلة، ثم:

```sh
# بناء sidecar إنتاجي (الجسر مُفعّل، الـ controller محذوف)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# جسر فقط (واجهة الـ controller محتفظ بها للتطوير)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### التحقق

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

آخر تشغيل على هذا الفرع: 0 خطأ / 189 unit / 6 + 8 تكاملي / 38 + 4 build_support.

## الأسرار و CI

يحتاج الجسر خمسة مدخلات وقت البناء:

| المتغير | الغرض | الصيغة |
|---|---|---|
| `HBBS_KEY` | المفتاح العام لخادم rendezvous (يطغى على `RS_PUB_KEY`) | base64، بعد فك التشفير 32 بايت |
| `HBBS_HOST` | مضيف خادم rendezvous | `host[:port[-port2]]` |
| `HBBR_HOST` | مضيف خادم relay | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (أو `_B64`) | سر HMAC مشترك بطول 32 بايت | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | إصدار تدوير المفتاح المتزايد | عدد صحيح غير سالب |

طريقتان:

1. **التطوير المحلي** &mdash; املأ `secret.sec` في جذر المستودع بأسطر `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. الملف يُتجاهل بواسطة [`.gitignore`](../.gitignore).
2. **CI** &mdash; اضبط الأسماء نفسها بصفتها أسرار مستودع في GitHub Actions؛ يحقنها [`.github/workflows/build.yml`](../.github/workflows/build.yml) كمتغيرات بيئة مُقنَّعة. **لا يُجسَّد `secret.sec` على runners أبداً**.

كلٌ من `secret.sec` و `vhd_bridge_secret.bin` موجود في `.gitignore` و **يجب ألا يُرفَع** إلى git أبداً. `scripts/check_bridge_strings.ps1` هي شبكة الأمان بعد البناء.

## الترخيص والإسناد

تُوزَّع هذه النسخة بنفس ترخيص الأصل: **GNU Affero General Public License v3.0 (AGPL-3.0)**. النص الكامل في [`../LICENCE`](../LICENCE)؛ النسخة **لا تعدّل** الترخيص.

- جميع حقوق التأليف على قاعدة كود RustDesk الأصلية تبقى لمؤلفي الأصل والمساهمين فيه، انظر <https://github.com/rustdesk/rustdesk>.
- التعديلات التي تقدمها هذه النسخة (ميزتا `vhd-bridge` / `controlled-only` والكود المساند) تُوزَّع أيضاً تحت AGPL-3.0؛ يحتفظ المستخدمون النهائيون بكل الحقوق التي يمنحها AGPL-3.0، بما فيها الحق في الكود المصدري المرافق لأي نشر شبكي.
- اسم وشعار "RustDesk" ملك للمشروع الأصلي؛ تستخدمهما النسخة فقط لتعريف قاعدة الكود المعدلة، وفقاً للاستخدام العادل للعلامات التجارية في نسخ البرامج الحرة.
- مكتبات الأطراف الثالثة (vcpkg: `libvpx`، `libyuv`، `opus`، `aom`؛ Sciter SDK؛ تبعيات Flutter) تحتفظ بتراخيصها الأصلية.

استخدامك لهذه النسخة يعني قبول AGPL-3.0 و**إخلاء مسؤولية الاستخدام السيئ** في أعلى الملف.
