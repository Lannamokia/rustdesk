<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> dengan jembatan otentikasi mesin VHD</b><br>
  <a href="#status-fork">Status fork</a> &bull;
  <a href="#yang-ditambahkan-fork-ini">Tambahan</a> &bull;
  <a href="#kompilasi">Kompilasi</a> &bull;
  <a href="#rahasia-dan-ci">Rahasia &amp; CI</a> &bull;
  <a href="#lisensi-dan-atribusi">Lisensi</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Repository ini adalah fork hilir dari [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Dokumentasi Inggris lengkap: [`../README.md`](../README.md).
> Hak cipta, merek dagang, dan lisensi AGPL-3.0 upstream tidak berubah &mdash; lihat [Lisensi dan atribusi](#lisensi-dan-atribusi).

> [!Caution]
> **Penafian penyalahgunaan:** developer upstream RustDesk dan pemelihara fork ini tidak menoleransi atau mendukung penggunaan tidak etis atau ilegal dari perangkat lunak ini. Akses, kontrol, atau pelanggaran privasi tanpa izin dilarang keras. Penulis tidak bertanggung jawab atas penyalahgunaan apa pun.

---

## Status fork

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (di git sebagai remote `upstream`) |
| **Fork ini** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Branch aktif** | `feature/vhd-machine-auth-bridge` |
| **Submodul** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), branch sama |
| **Lisensi** | AGPL-3.0 (sama dengan upstream &mdash; lihat [`LICENCE`](../LICENCE)) |
| **Tujuan** | Menjalankan sisi Controlled RustDesk sebagai sidecar agen VHDMount eksternal melalui jembatan terotentikasi yang terikat ke mesin. |

Saat `vhd-bridge` **dimatikan**, artefak build berperilaku identik dengan RustDesk upstream &mdash; invarian ini diverifikasi otomatis oleh `tests/feature_off_parity.rs`.

## Yang ditambahkan fork ini

Subsistem kohesif &mdash; **jembatan otentikasi mesin VHD** &mdash; dikendalikan oleh dua Cargo feature, **default mati**:

- **`vhd-bridge`** &mdash; mengompilasi worker jembatan, kabel IPC, overlay UI pemeliharaan, dan smoke test.
- **`controlled-only`** &mdash; menghapus UI dan jalur kode Controller (initiator), menghasilkan biner yang hanya dapat dikontrol; dipasangkan dengan `vhd-bridge` untuk build sidecar produksi.

Tanpa feature aktif, `cargo run` dan alur build upstream berjalan tanpa perubahan.

### Perubahan utama

- **`src/vhd_bridge/`** &ndash; worker named pipe, mesin status `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 dengan rahasia bersama 32 byte yang disuntikkan saat build, backoff koneksi ulang, observabilitas terstruktur, log sink yang menyensor rahasia.
- **`src/server/connection.rs`** &ndash; gerbang persetujuan: sebelum menerima peer masuk, dikonsultasikan peer-set otentikasi mesin yang dipelihara oleh jembatan.
- **`src/auth_2fa.rs`** &ndash; 2FA dipaksa OFF selagi jembatan menguasai otentikasi (diverifikasi oleh `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay yang merefleksikan status jembatan (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate pendukung yang dibagi `build.rs` dan CI: gerbang prasyarat ketat, parser `secret.sec` toleran, uji konsistensi terhadap dokumen protokol.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; referensi wire protocol.
- **`scripts/check_bridge_strings.ps1`** &ndash; pemindai kebocoran pasca-build: memastikan tidak ada byte plaintext `HBBS Key` / `VHDMount Key` bocor ke artefak.
- **`.github/workflows/vhd-bridge.yml`** &mdash; matriks CI yang membangun artefak Windows feature-on / feature-off / controlled-only.

Spesifikasi lengkap di [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Klon

Fork mengubah URL submodul `libs/hbb_common`; klon rekursif:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Kalau sudah clone dengan `.gitmodules` upstream: `git submodule sync && git submodule update --init --recursive`.

## Kompilasi

### Build upstream (tanpa jembatan)

Tanpa feature aktif, fork adalah superset ketat dari upstream; instruksi upstream berlaku tanpa perubahan. Dependensi dan perintah lengkap di [`../README.md`](../README.md).

### Build dengan jembatan (Windows MSVC, direkomendasikan)

Jembatan saat ini hanya mendukung Windows (transport named pipe dan agen VHDMount).

Lingkungan yang diperlukan:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <jalur LLVM\x64\bin>
```

Lalu isi `secret.sec` (khusus dev) atau set variabel env terkait, lalu:

```sh
# Build sidecar produksi (jembatan ON, controller dihapus)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Hanya jembatan (UI controller dipertahankan untuk dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verifikasi

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Run terakhir di branch ini: 0 error / 189 unit / 6 + 8 integrasi / 38 + 4 build_support.

## Rahasia dan CI

Jembatan butuh lima input saat build:

| Variabel | Tujuan | Format |
|---|---|---|
| `HBBS_KEY` | Public key rendezvous server (menimpa `RS_PUB_KEY`) | base64, 32 byte setelah dekode |
| `HBBS_HOST` | Host rendezvous server | `host[:port[-port2]]` |
| `HBBR_HOST` | Host relay server | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (atau `_B64`) | Rahasia HMAC bersama 32 byte | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Versi rotasi kunci monotonik | bilangan bulat non-negatif |

Dua jalur:

1. **Dev lokal** &mdash; isi `secret.sec` di root repo dengan `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. File diabaikan oleh [`.gitignore`](../.gitignore).
2. **CI** &mdash; konfigurasi nama yang sama sebagai repository secret GitHub Actions; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) menyuntikkannya sebagai variabel env yang dimasking. **`secret.sec` tidak pernah dimaterialkan di runner**.

`secret.sec` dan `vhd_bridge_secret.bin` keduanya ada di `.gitignore` dan **tidak boleh di-commit**. `scripts/check_bridge_strings.ps1` adalah jaring pengaman pasca-build.

## Lisensi dan atribusi

Fork didistribusikan dengan lisensi yang sama dengan upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Teks lengkap di [`../LICENCE`](../LICENCE); fork **tidak memodifikasi** lisensi.

- Semua hak cipta atas basis kode RustDesk upstream tetap pada penulis dan kontributor upstream, lihat <https://github.com/rustdesk/rustdesk>.
- Modifikasi yang dibawa fork ini (feature `vhd-bridge` / `controlled-only` dan kode pendukung) juga didistribusikan di bawah AGPL-3.0; pengguna hilir mempertahankan semua hak yang diberikan AGPL-3.0, termasuk hak atas kode sumber yang sesuai untuk setiap penyebaran jaringan.
- Nama dan logo "RustDesk" milik proyek upstream; fork hanya menggunakannya untuk mengidentifikasi basis kode yang dimodifikasi, sesuai praktik penggunaan wajar merek dagang untuk fork perangkat lunak bebas.
- Pustaka pihak ketiga (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; dependensi Flutter) tetap dengan lisensi aslinya.

Menggunakan fork ini berarti menerima AGPL-3.0 dan **penafian penyalahgunaan** di bagian atas berkas.
