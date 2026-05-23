<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> con el puente de autenticación de máquina VHD</b><br>
  <a href="#estado-del-fork">Estado del fork</a> &bull;
  <a href="#qué-añade-este-fork">Qué añade</a> &bull;
  <a href="#compilación">Compilación</a> &bull;
  <a href="#secretos-y-ci">Secretos &amp; CI</a> &bull;
  <a href="#licencia-y-atribución">Licencia</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Este repositorio es un fork descendente de [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Documentación inglesa completa: [`../README.md`](../README.md).
> Los derechos de autor, marcas y la licencia AGPL-3.0 originales permanecen sin cambios &mdash; ver [Licencia y atribución](#licencia-y-atribución).

> [!Caution]
> **Aviso sobre uso indebido:** los desarrolladores originales de RustDesk y los mantenedores de este fork no aprueban ni respaldan ningún uso no ético o ilegal de este software. El uso indebido &mdash; acceso, control o invasión de privacidad sin autorización &mdash; está estrictamente prohibido. Los autores no se responsabilizan de ningún uso indebido.

---

## Estado del fork

| | |
|---|---|
| **Origen (upstream)** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (configurado como remote `upstream` en git) |
| **Este fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Rama activa** | `feature/vhd-machine-auth-bridge` |
| **Submódulo** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), misma rama |
| **Licencia** | AGPL-3.0 (sin cambios respecto al origen &mdash; ver [`LICENCE`](../LICENCE)) |
| **Objetivo** | Hacer que el lado controlado de RustDesk funcione como sidecar del agente VHDMount externo a través de un puente autenticado y vinculado a la máquina. |

Cuando `vhd-bridge` está **desactivado**, el artefacto compilado se comporta igual que RustDesk upstream &mdash; este invariante lo verifica automáticamente `tests/feature_off_parity.rs`.

## Qué añade este fork

Un subsistema cohesivo &mdash; el **puente de autenticación de máquina VHD** &mdash; controlado por dos *features* de Cargo, **desactivadas por defecto**:

- **`vhd-bridge`** &mdash; compila el worker del puente, el cableado IPC, la UI de overlay de mantenimiento y los tests de smoke.
- **`controlled-only`** &mdash; elimina la UI y los caminos de código del Controlador (initiator), produciendo una binary que solo puede ser controlada; combinable con `vhd-bridge` para la build sidecar de producción.

Sin features activas, `cargo run` y el flujo upstream funcionan idénticamente.

### Cambios principales

- **`src/vhd_bridge/`** &ndash; worker de named pipe, máquina de estados `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 con secreto compartido de 32 bytes inyectado en compilación, backoff de reconexión, observabilidad estructurada, log sink que oculta los secretos.
- **`src/server/connection.rs`** &ndash; puerta de aprobación: antes de aceptar un peer entrante, consulta el conjunto de peers de autenticación de máquina mantenido por el puente.
- **`src/auth_2fa.rs`** &ndash; 2FA forzado a OFF mientras el puente gobierna la autenticación (verificado por `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay que refleja el estado del puente (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate auxiliar compartida entre `build.rs` y CI: puerta de prerrequisitos estricta, parser tolerante para `secret.sec`, test de coherencia con la doc del protocolo.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; referencia del protocolo de cable.
- **`scripts/check_bridge_strings.ps1`** &ndash; escáner post-build que asegura que no se filtren bytes en claro de `HBBS Key` / `VHDMount Key` en los artefactos.
- **`.github/workflows/vhd-bridge.yml`** &mdash; matriz de CI que compila los artefactos Windows feature-on / feature-off / controlled-only.

Diseño completo en [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Clonar

El fork cambia la URL del submódulo `libs/hbb_common`, así que clona en modo recursivo:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Si ya clonaste con el `.gitmodules` upstream: `git submodule sync && git submodule update --init --recursive`.

## Compilación

### Build upstream (sin puente)

Sin features activas este fork es un superconjunto estricto del upstream; las instrucciones upstream aplican sin cambios. Dependencias y comandos completos en [`../README.md`](../README.md).

### Build con puente (Windows MSVC, recomendado)

El puente solo soporta Windows actualmente (transporte de named pipe y agente VHDMount).

Entorno requerido:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <ruta a LLVM\x64\bin>
```

Después, rellena `secret.sec` (dev-only) o define las variables de entorno equivalentes, y:

```sh
# Build sidecar de producción (puente ON, controlador eliminado)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Solo puente (UI de controlador conservada para iteración de dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verificación

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Última ejecución en esta rama: 0 errores / 189 tests unitarios / 6 + 8 de integración / 38 + 4 de build_support.

## Secretos y CI

El puente requiere cinco entradas en tiempo de compilación:

| Variable | Función | Formato |
|---|---|---|
| `HBBS_KEY` | Clave pública del rendezvous server (sobrescribe `RS_PUB_KEY`) | base64, 32 bytes tras decodificar |
| `HBBS_HOST` | Host del rendezvous server | `host[:port[-port2]]` |
| `HBBR_HOST` | Host del relay server | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (o `_B64`) | Secreto compartido HMAC de 32 bytes | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Versión monótona de rotación de clave | entero no negativo |

Dos vías:

1. **Dev local** &mdash; rellenar `secret.sec` en la raíz con `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. El fichero está ignorado por [`.gitignore`](../.gitignore).
2. **CI** &mdash; configurar los mismos nombres como secretos de repositorio en GitHub Actions; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) los inyecta como variables enmascaradas. **`secret.sec` nunca se materializa en los runners**.

`secret.sec` y `vhd_bridge_secret.bin` están en `.gitignore` y **no deben commitearse jamás**. `scripts/check_bridge_strings.ps1` es la red de seguridad post-build.

## Licencia y atribución

Este fork se distribuye bajo la misma licencia que el upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Texto completo en [`../LICENCE`](../LICENCE); este fork **no la modifica**.

- Todos los derechos de autor del código RustDesk upstream pertenecen a sus autores y contribuyentes upstream, ver <https://github.com/rustdesk/rustdesk>.
- Las modificaciones de este fork (features `vhd-bridge` y `controlled-only` con su código de soporte) también se distribuyen bajo AGPL-3.0; los usuarios descendentes conservan todos los derechos otorgados por la AGPL-3.0, incluido el derecho a recibir el código fuente correspondiente para cualquier despliegue en red.
- El nombre y el logotipo "RustDesk" pertenecen al proyecto upstream; este fork los utiliza únicamente para identificar la base de código modificada, dentro del uso justo de marcas en forks de software libre.
- Las bibliotecas de terceros (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; SDK de Sciter; dependencias de Flutter) conservan sus licencias originales.

Usar este fork implica aceptar la AGPL-3.0 y el **aviso sobre uso indebido** del comienzo de este archivo.
