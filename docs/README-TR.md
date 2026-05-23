<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; VHD makine-kimliklendirme köprülü <code>Lannamokia</code> fork'u</b><br>
  <a href="#fork-durumu">Fork durumu</a> &bull;
  <a href="#bu-fork-ne-ekliyor">Eklenenler</a> &bull;
  <a href="#derleme">Derleme</a> &bull;
  <a href="#sırlar-ve-ci">Sırlar &amp; CI</a> &bull;
  <a href="#lisans-ve-atıf">Lisans</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Bu depo [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) projesinin downstream forkudur. Tam İngilizce belge: [`../README.md`](../README.md).
> Upstream telif hakları, ticari markalar ve AGPL-3.0 lisansı değişmemiştir &mdash; bkz. [Lisans ve atıf](#lisans-ve-atıf).

> [!Caution]
> **Kötüye kullanım uyarısı:** upstream RustDesk geliştiricileri ve bu fork'un bakımcıları yazılımın etik dışı veya yasal olmayan kullanımını desteklemez. Yetkisiz erişim, denetim veya gizlilik ihlali kesinlikle yasaktır. Yazarlar uygulamanın kötüye kullanımından sorumlu değildir.

---

## Fork durumu

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (git'te `upstream` remote olarak ayarlı) |
| **Bu fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Aktif dal** | `feature/vhd-machine-auth-bridge` |
| **Alt modül** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), aynı dal |
| **Lisans** | AGPL-3.0 (upstream ile aynı &mdash; bkz. [`LICENCE`](../LICENCE)) |
| **Amaç** | RustDesk denetlenen tarafını dış VHDMount ajansının sidecar'ı olarak, makineye sabitlenmiş kimliklendirilmiş bir köprü üzerinden çalıştırmak. |

`vhd-bridge` **kapalıyken** derleme çıktısı upstream RustDesk ile davranışsal olarak özdeştir &mdash; bu değişmez `tests/feature_off_parity.rs` ile otomatik doğrulanır.

## Bu fork ne ekliyor

Tek bir tutarlı alt sistem &mdash; **VHD makine-kimliklendirme köprüsü** &mdash; **varsayılan olarak kapalı** iki Cargo feature ile yönetilir:

- **`vhd-bridge`** &mdash; köprü worker'ı, IPC bağlantıları, bakım UI overlay'i ve smoke testlerini derler.
- **`controlled-only`** &mdash; Controller (initiator) UI ve kod yollarını çıkararak yalnızca-denetlenebilir binary üretir; üretim sidecar build'i için `vhd-bridge` ile birlikte kullanılır.

Hiçbir feature aktif değilken `cargo run` ve upstream build akışı aynen çalışır.

### Başlıca değişiklikler

- **`src/vhd_bridge/`** &ndash; named-pipe worker, `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval` durum makinesi, build sırasında enjekte edilen 32 baytlık ortak sırla HMAC-SHA256, yeniden bağlanma backoff'u, yapılandırılmış gözlemlenebilirlik, sırları redakte eden log sink.
- **`src/server/connection.rs`** &ndash; onay kapısı: gelen peer kabul edilmeden önce köprünün makine-kimliklendirme peer kümesi sorgulanır.
- **`src/auth_2fa.rs`** &ndash; köprü kimlik doğrulamayı yönettiği sürece 2FA zorla kapatılır (`tests/smoke_2fa_disabled.rs` doğrular).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; köprü durumunu (`active / starting / lost`) yansıtan overlay.
- **`libs/build_support/`** &ndash; `build.rs` ve CI tarafından paylaşılan yardımcı crate: katı önkoşul kapısı, hoşgörülü `secret.sec` parser'ı, protokol dokümanıyla tutarlılık testi.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; kablolu protokol referansı.
- **`scripts/check_bridge_strings.ps1`** &ndash; build sonrası sızıntı tarayıcı: çıktılarda açık metin `HBBS Key` / `VHDMount Key` baytlarının olmadığını garanti eder.
- **`.github/workflows/vhd-bridge.yml`** &mdash; feature-on / feature-off / controlled-only Windows artifact'larını derleyen CI matrisi.

Tam belirtim: [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klonlama

Fork `libs/hbb_common` alt modül URL'sini değiştirir; özyinelemeli klonla:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Önceden upstream `.gitmodules` ile klonlandıysa: `git submodule sync && git submodule update --init --recursive`.

## Derleme

### Upstream build (köprüsüz)

Hiçbir feature aktif değilken fork upstream'in katı bir üst kümesidir; upstream talimatları aynen geçerlidir. Tam bağımlılıklar ve komutlar [`../README.md`](../README.md) içinde.

### Köprülü build (Windows MSVC, önerilen)

Köprü şu anda yalnızca Windows'u destekler (named-pipe taşıma ve VHDMount ajansı).

Gereken ortam:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <LLVM\x64\bin yolu>
```

Sonra dev-only `secret.sec`'i doldur veya ilgili env değişkenlerini ayarla, sonra:

```sh
# Üretim sidecar build'i (köprü ON, controller çıkarılmış)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Yalnızca köprü (controller UI dev için korunur)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Doğrulama

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Bu daldaki son çalışma: 0 hata / 189 unit / 6 + 8 entegrasyon / 38 + 4 build_support.

## Sırlar ve CI

Köprü beş build-zamanı girdi ister:

| Değişken | Amaç | Format |
|---|---|---|
| `HBBS_KEY` | Rendezvous sunucusu ortak anahtarı (`RS_PUB_KEY`'i ezer) | base64, çözüldükten sonra 32 bayt |
| `HBBS_HOST` | Rendezvous sunucusu host | `host[:port[-port2]]` |
| `HBBR_HOST` | Relay sunucusu host | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (veya `_B64`) | 32 baytlık ortak HMAC sırrı | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Monoton anahtar rotasyon sürümü | negatif olmayan tamsayı |

İki yol:

1. **Yerel dev** &mdash; depo kökündeki `secret.sec`'i `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:` ile doldur. Dosya [`.gitignore`](../.gitignore) ile yok sayılır.
2. **CI** &mdash; aynı adlarla GitHub Actions repository secret olarak yapılandır; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) maskeli env değişkenleri olarak enjekte eder. **`secret.sec` runner'larda asla somutlaştırılmaz**.

`secret.sec` ve `vhd_bridge_secret.bin` her ikisi de `.gitignore`'dadır ve **kesinlikle commit edilmemelidir**. `scripts/check_bridge_strings.ps1` build sonrası emniyet ağıdır.

## Lisans ve atıf

Bu fork upstream ile aynı lisans altındadır: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Tam metin: [`../LICENCE`](../LICENCE); fork lisansı **değiştirmez**.

- Upstream RustDesk kod tabanına ait tüm telif hakları upstream yazarları ve katkı sağlayıcılarındadır, bkz. <https://github.com/rustdesk/rustdesk>.
- Bu forkun getirdiği değişiklikler (`vhd-bridge` / `controlled-only` feature'ları ve destek kodu) da AGPL-3.0 altında dağıtılır; downstream kullanıcılar AGPL-3.0'ın verdiği tüm hakları, ağ üzerinden dağıtım için karşılık gelen kaynak kod hakkını da korur.
- "RustDesk" adı ve logosu upstream projesine aittir; fork bunları yalnızca değiştirilen kod tabanını tanımlamak için kullanır; bu, özgür yazılım fork'larında ticari marka adil kullanımına uygundur.
- Üçüncü taraf kütüphaneler (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter bağımlılıkları) orijinal lisanslarını korur.

Bu fork'u kullanmak AGPL-3.0 koşullarını ve dosyanın başındaki **kötüye kullanım uyarısını** kabul etmek anlamına gelir.
