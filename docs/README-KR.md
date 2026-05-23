<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code> 포크 (VHD machine-auth bridge)</b><br>
  <a href="#포크-상태">포크 상태</a> &bull;
  <a href="#이-포크가-추가하는-것">추가 사항</a> &bull;
  <a href="#빌드">빌드</a> &bull;
  <a href="#비밀-값과-ci">비밀 값과 CI</a> &bull;
  <a href="#라이선스와-저작권">라이선스</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> 본 저장소는 [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk)의 다운스트림 포크입니다. 전체 영문 문서는 [`../README.md`](../README.md)를 참고하세요.
> 상류의 저작권, 상표, AGPL-3.0 라이선스는 변경되지 않았습니다 &mdash; [라이선스와 저작권](#라이선스와-저작권) 참고.

> [!Caution]
> **오용 면책:** 상류 RustDesk 개발자와 본 포크의 유지자는 본 소프트웨어의 비윤리적 또는 불법적 사용을 용인하거나 지원하지 않습니다. 무단 접근, 제어, 프라이버시 침해 등의 오용은 엄격히 금지됩니다. 저작자는 응용 프로그램의 어떠한 오용에도 책임지지 않습니다.

---

## 포크 상태

| | |
|---|---|
| **상류** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (git 에서 `upstream` 리모트) |
| **본 포크** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **활성 브랜치** | `feature/vhd-machine-auth-bridge` |
| **서브모듈** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), 동일 브랜치 |
| **라이선스** | AGPL-3.0 (상류와 동일, [`LICENCE`](../LICENCE) 참고) |
| **목적** | RustDesk 피제어 측을 외부 VHDMount 에이전트의 사이드카로 동작시키며, 인증되고 머신에 결박된 브리지로 통신합니다. |

`vhd-bridge`가 **꺼져 있을** 때 빌드 산출물은 상류 RustDesk와 행위적으로 동일하며, `tests/feature_off_parity.rs`가 이를 자동 검증합니다.

## 이 포크가 추가하는 것

응집된 하위 시스템 하나 &mdash; **VHD machine-auth bridge** &mdash; 를 도입했고, **기본값 꺼짐**인 두 Cargo feature로 제어됩니다:

- **`vhd-bridge`** &mdash; 브리지 워커, IPC 배선, 유지보수 오버레이 UI, smoke 테스트를 컴파일에 포함합니다.
- **`controlled-only`** &mdash; Controller(initiator) UI와 코드 경로를 제거하여 피제어 전용 바이너리를 생성합니다. 운영용 사이드카 빌드는 `vhd-bridge`와 함께 사용합니다.

어느 feature도 켜지 않으면 기존 `cargo run` 및 상류 빌드 흐름이 그대로 유지됩니다.

### 주요 변경 사항

- **`src/vhd_bridge/`** &ndash; 네임드 파이프 워커. 상태 기계 `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, 빌드 시 주입된 32 바이트 공유 비밀로 HMAC-SHA256, 재접속 백오프, 구조화된 관측성, 비밀 값을 마스킹하는 로그 싱크.
- **`src/server/connection.rs`** &ndash; 들어오는 피어를 수락하기 전에 브리지의 machine-auth peer set에 질의하는 승인 게이트.
- **`src/auth_2fa.rs`** &ndash; 브리지가 인증을 담당하는 동안 2FA를 강제로 비활성화 (`tests/smoke_2fa_disabled.rs`로 검증).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; 브리지 상태(`active / starting / lost`)를 반영하는 유지보수 오버레이.
- **`libs/build_support/`** &ndash; `build.rs`와 CI가 공유하는 보조 crate. 사전요건 변수 엄격 게이트, `secret.sec` 관용 파서, 프로토콜 문서 무결성 테스트.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; 와이어 프로토콜 명세서.
- **`scripts/check_bridge_strings.ps1`** &ndash; 빌드 후 누출 스캐너. `HBBS Key` / `VHDMount Key` 평문이 바이너리에 남지 않음을 보증.
- **`.github/workflows/vhd-bridge.yml`** &mdash; feature-on / feature-off / controlled-only 세 가지 Windows 산출물을 빌드하는 CI 매트릭스.

전체 설계 문서는 [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge)에 있습니다.

## 클론

본 포크는 `libs/hbb_common` 서브모듈 URL을 수정했으므로 재귀 클론이 필요합니다:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

이미 상류 `.gitmodules`로 클론한 적이 있다면 `git submodule sync && git submodule update --init --recursive`를 실행하세요.

## 빌드

### 상류 빌드 (브리지 끔)

feature를 켜지 않으면 본 포크는 상류의 엄격한 상위 집합이며, 상류 빌드 안내가 그대로 적용됩니다. 전체 의존성과 명령은 [`../README.md`](../README.md)를 참고하세요.

### 브리지 빌드 (Windows MSVC, 권장)

브리지는 현재 Windows만 지원합니다 (네임드 파이프 전송과 VHDMount 에이전트 의존성).

필요한 환경 변수:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <LLVM\x64\bin 경로>
```

이후 개발용 `secret.sec`를 채우거나 관련 환경 변수를 설정한 뒤:

```sh
# 운영 사이드카 빌드 (브리지 켜기 + Controller 제거)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# 브리지만 (개발 중 Controller UI 유지)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### 검증

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

본 브랜치 최근 실행 결과: 오류 0 / 단위 189 통과 / 통합 6 + 8 / build_support 38 + 4.

## 비밀 값과 CI

브리지는 빌드 시 5 가지 입력을 요구합니다:

| 변수 | 용도 | 형식 |
|---|---|---|
| `HBBS_KEY` | RustDesk rendezvous 서버 공개키 (`RS_PUB_KEY` 덮어쓰기) | base64, 디코드 후 32 바이트 |
| `HBBS_HOST` | rendezvous 서버 호스트 | `host[:port[-port2]]` |
| `HBBR_HOST` | relay 서버 호스트 | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (또는 `_B64`) | 32 바이트 HMAC 공유 비밀 | hex 64자 / base64 44자 |
| `VHD_BRIDGE_SECRET_VERSION` | 단조 증가하는 키 회전 버전 | 음이 아닌 정수 |

두 가지 공급 방법:

1. **로컬 개발** &mdash; 저장소 루트의 `secret.sec`에 `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`을 기록. 해당 파일은 [`.gitignore`](../.gitignore)에 의해 무시됩니다.
2. **CI** &mdash; GitHub Actions 저장소 비밀로 동일 이름을 등록. [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml)이 마스킹된 환경 변수로 주입하며, **`secret.sec`는 CI 러너에 절대 기록되지 않습니다**.

`secret.sec`와 `vhd_bridge_secret.bin`은 모두 `.gitignore`에 포함되어 있으며 **커밋 금지**입니다. `scripts/check_bridge_strings.ps1`는 빌드 후 최종 안전망입니다.

## 라이선스와 저작권

본 포크는 상류와 동일하게 **GNU Affero General Public License v3.0 (AGPL-3.0)**로 배포됩니다. 전문은 [`../LICENCE`](../LICENCE)에 있으며, 본 포크는 **수정하지 않습니다**.

- 상류 RustDesk 코드의 저작권은 모두 상류 RustDesk 저작자와 기여자에게 귀속됩니다 (<https://github.com/rustdesk/rustdesk>).
- 본 포크가 도입한 변경 사항(`vhd-bridge` / `controlled-only` 및 부속 코드) 또한 AGPL-3.0으로 배포됩니다. 다운스트림 사용자는 네트워크 배포 시 대응 소스 청구권을 포함한 AGPL-3.0이 부여하는 모든 권리를 보유합니다.
- "RustDesk" 이름과 로고는 상류 프로젝트에 귀속됩니다. 본 포크는 단지 수정 대상 코드를 식별하기 위해서만 사용하며, 이는 파생 자유 소프트웨어 프로젝트에서의 상표 공정 사용 관행을 따릅니다.
- vcpkg를 통해 도입된 제3자 라이브러리(`libvpx`, `libyuv`, `opus`, `aom`), Sciter SDK, Flutter 의존성 등은 각자의 원본 라이선스를 유지합니다.

본 포크 사용은 AGPL-3.0 조건과 상단의 **오용 면책**에 동의함을 의미합니다.
