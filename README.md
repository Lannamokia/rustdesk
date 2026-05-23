<p align="center">
  <img src="res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code> fork with the VHD machine-auth bridge</b><br>
  <a href="#fork-status">Fork status</a> &bull;
  <a href="#what-this-fork-adds">What this fork adds</a> &bull;
  <a href="#building">Building</a> &bull;
  <a href="#secrets-and-ci">Secrets &amp; CI</a> &bull;
  <a href="#repository-layout">Layout</a> &bull;
  <a href="#license-and-attribution">License</a><br>
  [<a href="docs/README-UA.md">Українська</a>] | [<a href="docs/README-CS.md">česky</a>] | [<a href="docs/README-ZH.md">中文</a>] | [<a href="docs/README-HU.md">Magyar</a>] | [<a href="docs/README-ES.md">Español</a>] | [<a href="docs/README-FA.md">فارسی</a>] | [<a href="docs/README-FR.md">Français</a>] | [<a href="docs/README-DE.md">Deutsch</a>] | [<a href="docs/README-PL.md">Polski</a>] | [<a href="docs/README-ID.md">Indonesian</a>] | [<a href="docs/README-FI.md">Suomi</a>] | [<a href="docs/README-ML.md">മലയാളം</a>] | [<a href="docs/README-JP.md">日本語</a>] | [<a href="docs/README-NL.md">Nederlands</a>] | [<a href="docs/README-IT.md">Italiano</a>] | [<a href="docs/README-RU.md">Русский</a>] | [<a href="docs/README-PTBR.md">Português (Brasil)</a>] | [<a href="docs/README-EO.md">Esperanto</a>] | [<a href="docs/README-KR.md">한국어</a>] | [<a href="docs/README-AR.md">العربي</a>] | [<a href="docs/README-VN.md">Tiếng Việt</a>] | [<a href="docs/README-DA.md">Dansk</a>] | [<a href="docs/README-GR.md">Ελληνικά</a>] | [<a href="docs/README-TR.md">Türkçe</a>] | [<a href="docs/README-NO.md">Norsk</a>] | [<a href="docs/README-RO.md">Română</a>]
</p>

> [!Important]
> This is a downstream fork of [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk).
> Upstream README translations remain available below; the **English** README is rewritten here to describe the fork's current state.
> All upstream copyrights, trademarks and the AGPL-3.0 license apply unchanged &mdash; see [License and attribution](#license-and-attribution).

> [!Caution]
> **Misuse Disclaimer:** The developers of RustDesk (upstream) and the maintainers of this fork do not condone or support any unethical or illegal use of this software. Misuse, such as unauthorized access, control or invasion of privacy, is strictly against our guidelines. The authors are not responsible for any misuse of the application.

---

## Fork status

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (tracked as the `upstream` git remote) |
| **This fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Active branch** | `feature/vhd-machine-auth-bridge` |
| **Submodule** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common) on the same branch |
| **License** | AGPL-3.0 (unchanged from upstream &mdash; see [`LICENCE`](LICENCE)) |
| **Goal** | Run the RustDesk Controlled side as a sidecar to the external VHDMount agent over an authenticated, machine-pinned bridge. |

When `vhd-bridge` is **disabled** the binary is byte-for-byte equivalent in behaviour to upstream RustDesk &mdash; this is enforced by `tests/feature_off_parity.rs`.

## What this fork adds

The fork introduces one cohesive subsystem &mdash; the **VHD machine-auth bridge** &mdash; gated behind two opt-in Cargo features:

- **`vhd-bridge`** &mdash; compiles in the bridge worker, IPC plumbing, the maintenance overlay, and the smoke tests.
- **`controlled-only`** &mdash; strips the Controller (initiator) UI and code paths so the resulting binary can only be controlled. Designed to be combined with `vhd-bridge` for the production sidecar build.

Both features default to **off**. Existing `cargo run` / upstream build flows are unchanged.

### Subsystem overview

- **`src/vhd_bridge/`** &ndash; named-pipe worker implementing `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`. HMAC-SHA256 over a build-time-injected 32-byte shared secret. Reconnect/backoff, structured observability, secret-redacting log sink.
- **`src/server/`, `src/server/connection.rs`** &ndash; peer-approval gate that consults the bridge's machine-auth peer set before accepting any incoming connection.
- **`src/auth_2fa.rs`** &ndash; 2FA is force-disabled while the bridge is governing authentication (see `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; UI overlay reflecting `active / starting / lost` bridge state.
- **`flutter/lib/desktop/pages/empty_initiator_page.dart`** &ndash; placeholder used when `controlled-only` strips the Controller pages.
- **`libs/build_support/`** &ndash; helper crate consumed by `build.rs` and CI:
  - Strict prerequisite gate requiring `HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST` and a 32-byte bridge secret when `vhd-bridge` is on.
  - Tolerant parser for the dev-only `secret.sec` file.
  - Doc-completeness check between `protocol.rs` and the wire reference.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; wire-protocol reference (frame layout, message catalogue, error codes, replay/timing rules).
- **`scripts/check_bridge_strings.ps1`** &ndash; post-build leakage scanner. Asserts `RS_PUB_KEY` consistency and that no plaintext `HBBS Key` / `VHDMount Key` bytes leak into shipped binaries.
- **`scripts/smoke_controlled_only.ps1`** &ndash; CI smoke harness for the `controlled-only` flavour.
- **`.github/workflows/vhd-bridge.yml`** &ndash; CI matrix building the feature-on / feature-off / controlled-only Windows artifacts and running the leakage + smoke scripts.

The full design lives under [`.kiro/specs/vhd-machine-auth-bridge/`](.kiro/specs/vhd-machine-auth-bridge) (requirements, design, tasks).

## Cloning

This fork updates the `libs/hbb_common` submodule URL, so use a recursive clone:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

If you previously cloned with the upstream `.gitmodules`, run:

```sh
git submodule sync
git submodule update --init --recursive
```

## Building

### Upstream build (no bridge)

This fork is a strict superset of upstream when `vhd-bridge` is off, so the upstream build instructions apply unchanged:

```sh
# Linux/macOS, after installing vcpkg + libvpx/libyuv/opus/aom (see below)
cargo run
```

See [Building the upstream way (Linux / macOS / Docker)](#building-the-upstream-way-linux--macos--docker) below for the full dependency list.

### Bridge-enabled build (Windows MSVC, recommended)

The bridge is currently Windows-only; the named-pipe transport and the VHDMount agent require Windows.

Required environment:

```text
VCPKG_ROOT             = C:\src\vcpkg              (or your vcpkg checkout)
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <path to LLVM\x64\bin>    (e.g. VS LLVM toolchain)
```

Then either populate the dev-only `secret.sec` file (see [Secrets and CI](#secrets-and-ci)) or set `HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST` / `VHD_BRIDGE_SECRET_HEX` (or `_B64`) / `VHD_BRIDGE_SECRET_VERSION` as env vars, and:

```sh
# Production sidecar build (bridge on, controller stripped)
cargo build --release \
  --features vhd-bridge,controlled-only \
  --target x86_64-pc-windows-msvc

# Bridge-only (keeps the controller UI for dev iteration)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verification

The full local verification matrix used during development:

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Latest run on this branch: 0 errors / 189 passing unit tests / 6 + 8 integration tests / 38 + 4 build_support tests.

### Building the upstream way (Linux / macOS / Docker)

These flows still work; the bridge stays compiled-out unless you pass the feature flags above.

#### Ubuntu 18 (Debian 10)

```sh
sudo apt install -y zip g++ gcc git curl wget nasm yasm libgtk-3-dev clang libxcb-randr0-dev libxdo-dev \
    libxfixes-dev libxcb-shape0-dev libxcb-xfixes0-dev libasound2-dev libpulse-dev cmake make \
    libclang-dev ninja-build libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libpam0g-dev
```

#### openSUSE Tumbleweed

```sh
sudo zypper install gcc-c++ git curl wget nasm yasm gcc gtk3-devel clang libxcb-devel libXfixes-devel \
    cmake alsa-lib-devel gstreamer-devel gstreamer-plugins-base-devel xdotool-devel pam-devel
```

#### Fedora 28 (CentOS 8)

```sh
sudo yum -y install gcc-c++ git curl wget nasm yasm gcc gtk3-devel clang libxcb-devel libxdo-devel \
    libXfixes-devel pulseaudio-libs-devel cmake alsa-lib-devel gstreamer1-devel \
    gstreamer1-plugins-base-devel pam-devel
```

#### Arch (Manjaro)

```sh
sudo pacman -Syu --needed unzip git cmake gcc curl wget yasm nasm zip make pkg-config clang gtk3 \
    xdotool libxcb libxfixes alsa-lib pipewire
```

#### Install vcpkg

```sh
git clone https://github.com/microsoft/vcpkg
cd vcpkg
git checkout 2023.04.15
cd ..
vcpkg/bootstrap-vcpkg.sh
export VCPKG_ROOT=$HOME/vcpkg
vcpkg/vcpkg install libvpx libyuv opus aom
```

On Fedora the libvpx build needs `-fPIC`; see the upstream README workaround in the
[upstream README history](https://github.com/rustdesk/rustdesk/blob/master/README.md) if you hit it.

#### Sciter dynamic library (legacy UI only)

The Sciter UI is deprecated upstream; the Flutter UI is the primary front-end. If you still need Sciter:

[Windows](https://raw.githubusercontent.com/c-smile/sciter-sdk/master/bin.win/x64/sciter.dll) &middot;
[Linux](https://raw.githubusercontent.com/c-smile/sciter-sdk/master/bin.lnx/x64/libsciter-gtk.so) &middot;
[macOS](https://raw.githubusercontent.com/c-smile/sciter-sdk/master/bin.osx/libsciter.dylib)

#### Docker

The upstream Dockerfile still works for the no-bridge build:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
docker build -t rustdesk-builder .
docker run --rm -it -v $PWD:/home/user/rustdesk \
    -v rustdesk-git-cache:/home/user/.cargo/git \
    -v rustdesk-registry-cache:/home/user/.cargo/registry \
    -e PUID="$(id -u)" -e PGID="$(id -g)" rustdesk-builder
```

## Secrets and CI

The bridge consumes five build-time inputs:

| Variable | Purpose | Format |
|---|---|---|
| `HBBS_KEY` | RustDesk rendezvous server public key (overrides `RS_PUB_KEY`) | base64, 32 bytes decoded |
| `HBBS_HOST` | Rendezvous server host | `host[:port[-port2]]` |
| `HBBR_HOST` | Relay server host | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (or `_B64`) | 32-byte HMAC shared secret | 64 hex chars / 44 base64 chars |
| `VHD_BRIDGE_SECRET_VERSION` | Monotonic key-rotation counter | non-negative integer |

Two paths are supported:

1. **Local dev** &mdash; populate `secret.sec` in the repo root with the lines `HBBS Key:`, `HBBS Host:`, `HBBR Host:`, `VHDMount Key:`, `VHDMount Key Version:`. The file is **git-ignored** (see [`.gitignore`](.gitignore)) and `build_support::parse_secret_sec` reads it.
2. **CI** &mdash; configure the same names as **GitHub Actions repository secrets** under `Settings &rarr; Secrets and variables &rarr; Actions`. The workflow at [`.github/workflows/vhd-bridge.yml`](.github/workflows/vhd-bridge.yml) injects them into the build via masked env vars; `secret.sec` is **never materialized on runners**.

Both `secret.sec` and `vhd_bridge_secret.bin` are listed in `.gitignore` and must never be committed. `scripts/check_bridge_strings.ps1` is the post-build safety net that scans shipped artifacts for plaintext key material.

## Controlled-side deployment

The `controlled-windows` workflow artifact (`rustdesk-controlled-windows-x86_64`) is a standalone `rustdesk.exe` built with `--features vhd-bridge,controlled-only`. The binary still understands the upstream RustDesk CLI surface for the controlled half (`--service`, `--server`, `--install-service`, `--cm`, `--tray`) but refuses every initiator subcommand at startup (`--connect`, `--port-forward`, etc).

### One-time install (recommended)

Run from an elevated PowerShell on the target Windows machine:

```powershell
# 1. Lay the binary down somewhere stable (NOT %TEMP%, NOT a path with `host=` / `licensed-` substrings — see `src/custom_server.rs`).
$dst = "C:\Program Files\RustDeskControlled"
New-Item -ItemType Directory -Force -Path $dst | Out-Null
Copy-Item .\rustdesk-controlled-windows-x86_64\rustdesk.exe $dst\rustdesk.exe -Force

# 2. Register the Windows service. This calls platform::install_service() which is
#    the same path the official MSI uses — service name `RustDesk`, ImagePath
#    `<dst>\rustdesk.exe --service`, start type Automatic, recovery actions set
#    to "restart on failure" by the install code.
& "$dst\rustdesk.exe" --install-service

# 3. (Optional) Verify the service is registered and running.
sc.exe query RustDesk
sc.exe qc RustDesk
```

Once installed, every subsequent boot:

1. `services.exe` launches `rustdesk.exe --service` as **LocalSystem**.
2. The `--service` process spawns `--server` in the active user's session (covers UAC-elevated screens, the lock screen, and fast-user-switching).
3. The `--server` process owns the named-pipe client to `VHDMount` (when present), the rendezvous connection to your hbbs, and the audio / video / clipboard / input services.

The service has built-in **automatic restart on crash** via the `failure` recovery actions installed by `install_service` &mdash; no external watchdog needed. If you want to confirm or tighten the policy:

```powershell
# Show current recovery actions
sc.exe qfailure RustDesk
# Tighten: restart with 5s delay on every failure for the first 24h
sc.exe failure RustDesk reset= 86400 actions= restart/5000/restart/5000/restart/5000
```

### Wiring up your self-hosted hbbs / hbbr

Compile-time injection (recommended, see [Secrets and CI](#secrets-and-ci)) bakes the host / port / public key into the binary so a fresh install connects out-of-the-box. If the artifact you deployed was built with `HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST` GitHub-Actions secrets populated, you do **not** need to touch any setting on the controlled machine.

If you need to point an existing install at a different rendezvous server post-deployment, edit `%APPDATA%\RustDesk\config\RustDesk2.toml` (the per-user options for the active session's `--server` process) **or** issue an IPC option update through `rustdesk.exe --option custom-rendezvous-server <host:port>`. Restart the service afterwards:

```powershell
sc.exe stop RustDesk
sc.exe start RustDesk
```

### Health checks

```powershell
# Service state.
sc.exe query RustDesk

# Live logs — the service writes to %ProgramData%\RustDesk\log when running as LocalSystem.
Get-Content "$env:ProgramData\RustDesk\log\service.log" -Tail 50 -Wait

# Bridge state (only present in vhd-bridge builds, requires VHDMount running).
# Reads the `vhd-bridge-state` IPC key — see docs/vhd-rustdesk-bridge-protocol.md §11.3 for the error-code vocabulary.
& "C:\Program Files\RustDeskControlled\rustdesk.exe" --get-option vhd-bridge-state

# Upstream-id-bound peer ID assigned by your hbbs.
& "C:\Program Files\RustDeskControlled\rustdesk.exe" --get-id
```

### Uninstall

```powershell
& "C:\Program Files\RustDeskControlled\rustdesk.exe" --uninstall-service
Remove-Item -Recurse -Force "C:\Program Files\RustDeskControlled"
Remove-Item -Recurse -Force "$env:ProgramData\RustDesk" -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force "$env:APPDATA\RustDesk"     -ErrorAction SilentlyContinue
```

### Notes on the `vhd-bridge` named-pipe peer

When the `vhd-bridge` feature is on (which it is in the `controlled-windows` artifact), `rustdesk.exe --server` will look for `VHDMount.exe` listening on `\\.\pipe\VHDMount.RustDeskBridge` (see [docs/vhd-rustdesk-bridge-protocol.md](docs/vhd-rustdesk-bridge-protocol.md) §2.1). If `VHDMount` is **not** running:

- The bridge worker stays in `Bridge_State == Initializing` and retries with backoff (Requirement 13.2 / 13.3).
- Inbound connections still work &mdash; the §19.8 fallback "password-correct = allow" path applies (no bridge-side approval).
- The `vhd-bridge-state` IPC key reports `vhd.bridge.failed.peer_not_vhdmount` so monitoring can alert.

Configuring / packaging `VHDMount` itself is out of scope for this repo &mdash; it ships separately. See `machine-auth.md` and `.kiro/specs/vhd-machine-auth-bridge/` for the cross-product spec.

## Repository layout

Bridge-specific:

- **[`src/vhd_bridge/`](src/vhd_bridge)** &mdash; bridge worker, frame codec, HMAC, peer approval, observability.
- **[`libs/build_support/`](libs/build_support)** &mdash; build-time prerequisite resolver, `secret.sec` parser, doc-completeness tests.
- **[`docs/vhd-rustdesk-bridge-protocol.md`](docs/vhd-rustdesk-bridge-protocol.md)** &mdash; wire-protocol reference.
- **[`scripts/`](scripts)** &mdash; PowerShell smoke + leakage scanners.
- **[`tests/feature_off_parity.rs`](tests/feature_off_parity.rs)**, **[`tests/smoke_2fa_disabled.rs`](tests/smoke_2fa_disabled.rs)**, **[`tests/vhd_bridge_integration.rs`](tests/vhd_bridge_integration.rs)** &mdash; integration suites.
- **[`.kiro/specs/vhd-machine-auth-bridge/`](.kiro/specs/vhd-machine-auth-bridge)** &mdash; full spec (requirements / design / tasks).
- **[`.github/workflows/vhd-bridge.yml`](.github/workflows/vhd-bridge.yml)** &mdash; bridge CI matrix.

Inherited from upstream (unchanged structurally):

- **[`libs/hbb_common`](libs/hbb_common)** &mdash; config, proto, tcp/udp, file-transfer fs helpers (this fork extends it on the same feature branch).
- **[`libs/scrap`](libs/scrap)** &mdash; screen capture.
- **[`libs/enigo`](libs/enigo)** &mdash; platform-specific keyboard/mouse control.
- **[`libs/clipboard`](libs/clipboard)** &mdash; cross-platform copy/paste.
- **[`src/server/`](src/server)** &mdash; audio / clipboard / input / video services and network connections.
- **[`src/client.rs`](src/client.rs)** &mdash; peer-connection initiation.
- **[`src/rendezvous_mediator.rs`](src/rendezvous_mediator.rs)** &mdash; talks to [`rustdesk-server`](https://github.com/rustdesk/rustdesk-server) for hole-punching / relay.
- **[`src/platform/`](src/platform)** &mdash; platform-specific code.
- **[`src/ui/`](src/ui)** &mdash; legacy Sciter UI (deprecated upstream).
- **[`flutter/`](flutter)** &mdash; Flutter UI for desktop and mobile (current front-end).

## Upstream resources

These all point at the upstream RustDesk project and apply to this fork's non-bridge surface area unchanged:

- Upstream chat: [Discord](https://discord.gg/nDceKgxnkV) &middot; [Twitter](https://twitter.com/rustdesk) &middot; [Reddit](https://www.reddit.com/r/rustdesk) &middot; [YouTube](https://www.youtube.com/@rustdesk)
- [**Upstream FAQ**](https://github.com/rustdesk/rustdesk/wiki/FAQ)
- [**Upstream binary releases**](https://github.com/rustdesk/rustdesk/releases) &mdash; this fork does **not** publish binaries
- [Upstream nightly builds](https://github.com/rustdesk/rustdesk/releases/tag/nightly)
- [Upstream contributing guide](docs/CONTRIBUTING.md)
- [RustDesk Server Pro](https://rustdesk.com/pricing.html)

## License and attribution

This fork is distributed under the same license as upstream RustDesk: **GNU Affero General Public License v3.0 (AGPL-3.0)**. The full text lives in [`LICENCE`](LICENCE) and is **not** modified by this fork.

- All copyrights to the upstream RustDesk codebase remain with the upstream RustDesk authors and contributors. See the upstream repository at <https://github.com/rustdesk/rustdesk>.
- Modifications introduced by this fork (the `vhd-bridge` and `controlled-only` features and supporting code) are likewise distributed under AGPL-3.0; downstream users retain all rights granted by AGPL-3.0, including the right to receive corresponding source for any networked deployment.
- The "RustDesk" name and logo are property of the upstream project; this fork uses them solely to identify the codebase being modified, in accordance with fair-use of trademarks for forked free-software projects.
- Bundled third-party libraries retain their original licenses (vcpkg-installed `libvpx`, `libyuv`, `opus`, `aom`; Sciter SDK; Flutter dependencies; etc.).

By using this fork you agree to be bound by the terms of AGPL-3.0 and the **Misuse Disclaimer** at the top of this file.

---

## Translations

The translated READMEs in `docs/README-*.md` have been rewritten alongside this English README to reflect the **fork's** behaviour, not upstream's. Use the language picker at the top of this file to navigate to them.
