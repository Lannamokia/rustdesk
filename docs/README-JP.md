<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code> fork（VHD machine-auth bridge 版）</b><br>
  <a href="#fork-の状態">Fork の状態</a> &bull;
  <a href="#この-fork-で追加されたもの">追加内容</a> &bull;
  <a href="#ビルド">ビルド</a> &bull;
  <a href="#シークレットと-ci">シークレットと CI</a> &bull;
  <a href="#ライセンスと帰属">ライセンス</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> このリポジトリは [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) の下流フォークです。完全な英語版ドキュメントは [`../README.md`](../README.md) を参照してください。
> 上流の著作権・商標・AGPL-3.0 ライセンスは変更されていません &mdash; [ライセンスと帰属](#ライセンスと帰属)を参照。

> [!Caution]
> **誤用に関する免責事項：** 上流 RustDesk の開発者および本 fork の保守者は、本ソフトウェアの非倫理的または違法な使用を容認・支援しません。無許可のアクセス、制御、プライバシー侵害といった誤用は厳に禁じられます。作成者はアプリケーションの誤用について一切責任を負いません。

---

## Fork の状態

| | |
|---|---|
| **上流** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk)（git 上では `upstream` リモート） |
| **本 fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **アクティブブランチ** | `feature/vhd-machine-auth-bridge` |
| **サブモジュール** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common)、同名ブランチ |
| **ライセンス** | AGPL-3.0（上流と同一、[`LICENCE`](../LICENCE) を参照） |
| **目的** | RustDesk の被制御側を、外部 VHDMount エージェントのサイドカーとして、認証済み・マシン固定のブリッジ経由で連携させること。 |

`vhd-bridge` を**無効**にした場合、ビルド成果物は上流 RustDesk と挙動上同一です &mdash; この不変条件は `tests/feature_off_parity.rs` で自動検証されます。

## この fork で追加されたもの

ひとつの一貫したサブシステム &mdash; **VHD machine-auth bridge** &mdash; を導入。**既定で無効**の 2 つの Cargo feature で制御されます：

- **`vhd-bridge`** &mdash; ブリッジ worker、IPC 配線、メンテナンスオーバーレイ UI、smoke テストをコンパイル時に組み込みます。
- **`controlled-only`** &mdash; Controller（initiator）側 UI とコードパスを除去し、被制御専用バイナリにします。`vhd-bridge` と組み合わせて本番サイドカーを構築する想定です。

どちらの feature も使わなければ、既存の `cargo run` および上流ビルドフローはそのまま動作します。

### 主要な変更点

- **`src/vhd_bridge/`** &ndash; 名前付きパイプ worker。状態機械 `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`、ビルド時注入の 32 バイト共有秘密による HMAC-SHA256、再接続バックオフ、構造化された可観測性、秘密値を秘匿する log sink。
- **`src/server/connection.rs`** &ndash; 受信ピアを受け入れる前に、ブリッジが配布する machine-auth peer set に問い合わせる承認ゲート。
- **`src/auth_2fa.rs`** &ndash; ブリッジが認証を担う間は 2FA を強制無効化（`tests/smoke_2fa_disabled.rs` で検証）。
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; ブリッジの状態（`active / starting / lost`）を反映するメンテナンス UI。
- **`libs/build_support/`** &ndash; `build.rs` と CI が共有するヘルパー crate。前提変数の厳格ゲート、`secret.sec` の寛容なパーサ、プロトコル文書との整合性テストを提供。
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; ワイヤープロトコル仕様書。
- **`scripts/check_bridge_strings.ps1`** &ndash; ビルド後の漏洩スキャナ。`HBBS Key` / `VHDMount Key` の平文がバイナリに残らないことを検証。
- **`.github/workflows/build.yml`** &mdash; ワークフロー（クロスプラットフォーム CI）。主な Windows ジョブは **controller-windows**（Flutter デスクトップバンドル、デフォルト features + `hwcodec` + `vram` + `flutter`、bridge なし）と **controlled-windows**（受動側サイドカー、`--features vhd-bridge,controlled-only,hwcodec,vram`）で、漏洩 + smoke スクリプトも実行する。

完全な設計ドキュメントは [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge) にあります。

## クローン

`libs/hbb_common` のサブモジュール URL が変更されているため、再帰クローンが必要です：

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

すでに上流の `.gitmodules` でクローン済みの場合は `git submodule sync && git submodule update --init --recursive` を実行してください。

## ビルド

### 上流ビルド（ブリッジ無効）

feature を有効化しない場合、本 fork は上流の厳密なスーパーセットなので、上流のビルド手順がそのまま使えます。完全な依存関係と手順は [`../README.md`](../README.md) を参照してください。

### ブリッジ有効ビルド（Windows MSVC、推奨）

ブリッジは現状 Windows のみサポート（名前付きパイプおよび VHDMount エージェントの制約）。

必要な環境変数：

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <LLVM\x64\bin のパス>
```

その後、開発用の `secret.sec` を用意するか、関連環境変数を設定して：

```sh
# 本番サイドカービルド（ブリッジ有効・Controller 除去）
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# ブリッジのみ（開発用に Controller UI を残す）
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### 検証

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

本ブランチでの直近実行：エラー 0 / unit 189 件パス / 統合 6 + 8 件 / build_support 38 + 4 件。

## シークレットと CI

ブリッジは 5 つのビルド時入力を要求します：

| 変数 | 用途 | 形式 |
|---|---|---|
| `HBBS_KEY` | RustDesk rendezvous サーバの公開鍵（`RS_PUB_KEY` を上書き） | base64、デコード後 32 バイト |
| `HBBS_HOST` | rendezvous サーバのホスト | `host[:port[-port2]]` |
| `HBBR_HOST` | relay サーバのホスト | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX`（または `_B64`） | 32 バイト HMAC 共有秘密 | hex 64 文字 / base64 44 文字 |
| `VHD_BRIDGE_SECRET_VERSION` | 単調増加の鍵ローテーション版番号 | 非負整数 |

供給経路は 2 種類：

1. **ローカル開発** &mdash; リポジトリルートに `secret.sec` を作成し `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:` を記載。同ファイルは [`.gitignore`](../.gitignore) で無視されます。
2. **CI** &mdash; GitHub Actions のリポジトリシークレットとして同名で登録。[`.github/workflows/build.yml`](../.github/workflows/build.yml) がマスクされた環境変数として注入し、**`secret.sec` は CI runner には決して書き出されません**。

`secret.sec` と `vhd_bridge_secret.bin` は両方とも `.gitignore` 入りで、**コミット禁止**です。`scripts/check_bridge_strings.ps1` がビルド後の最終防衛線です。

## ライセンスと帰属

本 fork は上流と同じ **GNU Affero General Public License v3.0 (AGPL-3.0)** で配布されます。全文は [`../LICENCE`](../LICENCE) にあり、本 fork は**改変していません**。

- 上流 RustDesk のコードベースに対する著作権はすべて上流 RustDesk の作者・コントリビューターに帰属します（<https://github.com/rustdesk/rustdesk>）。
- 本 fork が導入した変更（`vhd-bridge` / `controlled-only` および付随コード）も AGPL-3.0 で配布されます。下流の利用者は AGPL-3.0 が付与する全権利（ネットワーク提供時の対応ソース請求権を含む）を保持します。
- "RustDesk" の名称およびロゴは上流プロジェクトに帰属します。本 fork はあくまで「改変対象のコードベース」を識別する目的でのみ使用しており、これは派生フリーソフトウェアプロジェクトにおける商標のフェアユース慣行に従うものです。
- vcpkg 経由で取り込まれる第三者ライブラリ（`libvpx`、`libyuv`、`opus`、`aom`）、Sciter SDK、Flutter 依存物などは、それぞれのライセンスを保持します。

本 fork の使用は、AGPL-3.0 および冒頭の**免責事項**への同意を意味します。
