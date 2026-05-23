<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; bản fork <code>Lannamokia</code> kèm cầu xác thực máy VHD</b><br>
  <a href="#trạng-thái-fork">Trạng thái fork</a> &bull;
  <a href="#bản-fork-này-bổ-sung-gì">Bổ sung</a> &bull;
  <a href="#biên-dịch">Biên dịch</a> &bull;
  <a href="#bí-mật-và-ci">Bí mật &amp; CI</a> &bull;
  <a href="#giấy-phép-và-ghi-công">Giấy phép</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Repository này là bản fork hạ nguồn của [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Tài liệu tiếng Anh đầy đủ: [`../README.md`](../README.md).
> Bản quyền, thương hiệu và giấy phép AGPL-3.0 của upstream được giữ nguyên &mdash; xem [Giấy phép và ghi công](#giấy-phép-và-ghi-công).

> [!Caution]
> **Khước trách lạm dụng:** các nhà phát triển upstream RustDesk và những người duy trì bản fork này không dung thứ hay hỗ trợ bất kỳ hành vi sử dụng phi đạo đức hay bất hợp pháp nào của phần mềm. Truy cập, điều khiển hay xâm phạm quyền riêng tư trái phép đều bị nghiêm cấm. Tác giả không chịu trách nhiệm cho bất kỳ hành vi lạm dụng nào.

---

## Trạng thái fork

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (trong git là remote `upstream`) |
| **Bản fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Nhánh đang dùng** | `feature/vhd-machine-auth-bridge` |
| **Submodule** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), cùng tên nhánh |
| **Giấy phép** | AGPL-3.0 (giữ nguyên so với upstream &mdash; xem [`LICENCE`](../LICENCE)) |
| **Mục tiêu** | Chạy phía bị điều khiển của RustDesk như sidecar của tác nhân VHDMount bên ngoài qua một cầu được xác thực và gắn cứng vào máy. |

Khi `vhd-bridge` **tắt**, build artifact tương đương về hành vi với upstream RustDesk &mdash; bất biến này được kiểm chứng tự động bởi `tests/feature_off_parity.rs`.

## Bản fork này bổ sung gì

Một hệ con cô đọng &mdash; **cầu xác thực máy VHD** &mdash; được điều khiển bởi hai Cargo feature, **mặc định tắt**:

- **`vhd-bridge`** &mdash; biên dịch worker cầu, dây IPC, overlay UI bảo trì và smoke test vào.
- **`controlled-only`** &mdash; lược bỏ UI và đường code Controller (initiator) để tạo binary chỉ-bị-điều-khiển; kết hợp với `vhd-bridge` cho bản build sidecar sản xuất.

Không bật feature nào, `cargo run` và luồng build upstream vẫn vận hành như cũ.

### Thay đổi chính

- **`src/vhd_bridge/`** &ndash; worker named pipe, máy trạng thái `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 với bí mật chia sẻ 32 byte tiêm vào lúc build, backoff kết nối lại, observability có cấu trúc, log sink che bí mật.
- **`src/server/connection.rs`** &ndash; cổng phê duyệt: trước khi chấp nhận peer đến, tham vấn tập peer xác thực máy do cầu duy trì.
- **`src/auth_2fa.rs`** &ndash; 2FA bị buộc tắt khi cầu chỉ huy xác thực (kiểm chứng bởi `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay phản ánh trạng thái cầu (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate hỗ trợ dùng chung giữa `build.rs` và CI: cổng tiền điều kiện nghiêm ngặt, parser `secret.sec` khoan dung, kiểm tra nhất quán với tài liệu giao thức.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; tham chiếu giao thức đường truyền.
- **`scripts/check_bridge_strings.ps1`** &ndash; máy quét rò rỉ hậu build: bảo đảm không có byte văn bản gốc của `HBBS Key` / `VHDMount Key` lọt vào artifact.
- **`.github/workflows/vhd-bridge.yml`** &mdash; ma trận CI build artifact Windows feature-on / feature-off / controlled-only.

Đặc tả đầy đủ: [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Clone

Bản fork đổi URL submodule `libs/hbb_common`, hãy clone đệ quy:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Nếu trước đó đã clone bằng `.gitmodules` upstream: `git submodule sync && git submodule update --init --recursive`.

## Biên dịch

### Build upstream (không cầu)

Không bật feature, fork là tập trên nghiêm ngặt của upstream; hướng dẫn upstream áp dụng nguyên xi. Phụ thuộc và lệnh đầy đủ ở [`../README.md`](../README.md).

### Build có cầu (Windows MSVC, khuyến nghị)

Cầu hiện chỉ hỗ trợ Windows (named pipe và tác nhân VHDMount).

Môi trường yêu cầu:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <đường dẫn đến LLVM\x64\bin>
```

Sau đó điền `secret.sec` (chỉ dev) hoặc đặt biến môi trường tương ứng, rồi:

```sh
# Build sidecar sản xuất (cầu BẬT, controller bị bỏ)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Chỉ cầu (giữ UI controller để dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Kiểm chứng

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Lần chạy gần nhất trên nhánh: 0 lỗi / 189 unit / 6 + 8 tích hợp / 38 + 4 build_support.

## Bí mật và CI

Cầu yêu cầu năm đầu vào ở thời điểm build:

| Biến | Mục đích | Định dạng |
|---|---|---|
| `HBBS_KEY` | Khóa công khai rendezvous server (ghi đè `RS_PUB_KEY`) | base64, sau giải mã 32 byte |
| `HBBS_HOST` | Host rendezvous server | `host[:port[-port2]]` |
| `HBBR_HOST` | Host relay server | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (hoặc `_B64`) | Bí mật chia sẻ HMAC 32 byte | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Phiên bản xoay khóa đơn điệu | số nguyên không âm |

Hai con đường:

1. **Dev cục bộ** &mdash; điền `secret.sec` ở gốc repo với `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Tệp đã được [`.gitignore`](../.gitignore) bỏ qua.
2. **CI** &mdash; cùng tên đặt làm repository secret trong GitHub Actions; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) tiêm dưới dạng biến môi trường có mặt nạ. **`secret.sec` không bao giờ được hiện thực hóa trên runner**.

`secret.sec` và `vhd_bridge_secret.bin` đều ở `.gitignore` và **không được commit**. `scripts/check_bridge_strings.ps1` là lưới an toàn sau build.

## Giấy phép và ghi công

Bản fork phát hành theo cùng giấy phép upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Toàn văn ở [`../LICENCE`](../LICENCE); bản fork **không sửa đổi** giấy phép.

- Mọi bản quyền với mã nguồn upstream RustDesk thuộc về tác giả và người đóng góp upstream, xem <https://github.com/rustdesk/rustdesk>.
- Các sửa đổi mà bản fork giới thiệu (feature `vhd-bridge` / `controlled-only` và mã hỗ trợ) cũng phát hành theo AGPL-3.0; người dùng hạ nguồn giữ mọi quyền AGPL-3.0 cấp, kể cả quyền nhận mã nguồn tương ứng cho mọi triển khai mạng.
- Tên và logo "RustDesk" thuộc dự án upstream; bản fork chỉ dùng để định danh nền mã đã sửa, theo thông lệ sử dụng nhãn hiệu hợp lý cho fork phần mềm tự do.
- Thư viện bên thứ ba (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; phụ thuộc Flutter) giữ giấy phép gốc của chúng.

Sử dụng bản fork đồng nghĩa chấp thuận AGPL-3.0 và **khước trách lạm dụng** ở đầu tệp.
