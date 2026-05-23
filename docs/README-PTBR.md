<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> com a ponte de autenticação de máquina VHD</b><br>
  <a href="#status-do-fork">Status do fork</a> &bull;
  <a href="#o-que-este-fork-adiciona">Adições</a> &bull;
  <a href="#compilação">Compilação</a> &bull;
  <a href="#segredos-e-ci">Segredos &amp; CI</a> &bull;
  <a href="#licença-e-atribuição">Licença</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Este repositório é um fork descendente de [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Documentação inglesa completa: [`../README.md`](../README.md).
> Direitos autorais, marcas e licença AGPL-3.0 do upstream permanecem inalterados &mdash; ver [Licença e atribuição](#licença-e-atribuição).

> [!Caution]
> **Aviso sobre uso indevido:** os desenvolvedores upstream do RustDesk e os mantenedores deste fork não toleram nem apoiam qualquer uso antiético ou ilegal deste software. Acesso, controle ou invasão de privacidade não autorizados são estritamente proibidos. Os autores não se responsabilizam por uso indevido.

---

## Status do fork

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (configurado como remote `upstream` no git) |
| **Este fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Branch ativo** | `feature/vhd-machine-auth-bridge` |
| **Submódulo** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), mesmo branch |
| **Licença** | AGPL-3.0 (sem mudanças em relação ao upstream &mdash; ver [`LICENCE`](../LICENCE)) |
| **Objetivo** | Rodar o lado controlado do RustDesk como sidecar do agente externo VHDMount, através de uma ponte autenticada e fixada à máquina. |

Quando `vhd-bridge` está **desativado**, o artefato compilado é equivalente em comportamento ao RustDesk upstream &mdash; invariante verificado automaticamente por `tests/feature_off_parity.rs`.

## O que este fork adiciona

Um subsistema coeso &mdash; a **ponte de autenticação de máquina VHD** &mdash; controlado por duas features Cargo, **desativadas por padrão**:

- **`vhd-bridge`** &mdash; compila o worker da ponte, fiação IPC, overlay UI de manutenção e testes smoke.
- **`controlled-only`** &mdash; remove UI/caminhos de código do Controller (initiator) para gerar um binário só-controlado; combinado com `vhd-bridge` para a build sidecar de produção.

Sem nenhuma feature ativa, `cargo run` e o fluxo upstream funcionam identicamente.

### Principais mudanças

- **`src/vhd_bridge/`** &ndash; worker de named pipe, máquina de estados `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 com segredo de 32 bytes injetado em build time, backoff de reconexão, observabilidade estruturada, log sink que oculta segredos.
- **`src/server/connection.rs`** &ndash; gate de aprovação: antes de aceitar peer de entrada, consulta o conjunto de peers de autenticação de máquina mantido pela ponte.
- **`src/auth_2fa.rs`** &ndash; 2FA forçado a OFF enquanto a ponte governa autenticação (verificado por `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay refletindo estado da ponte (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate auxiliar compartilhada por `build.rs` e CI: gate rigoroso de pré-requisitos, parser tolerante para `secret.sec`, teste de coerência com a doc do protocolo.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; referência do protocolo de fio.
- **`scripts/check_bridge_strings.ps1`** &ndash; scanner pós-build que garante que bytes em claro de `HBBS Key` / `VHDMount Key` não vazem nos artefatos.
- **`.github/workflows/vhd-bridge.yml`** &mdash; matriz CI compilando os artefatos Windows feature-on / feature-off / controlled-only.

Especificação completa em [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Clonar

O fork modifica a URL do submódulo `libs/hbb_common`; clone recursivamente:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Se já clonou com o `.gitmodules` upstream: `git submodule sync && git submodule update --init --recursive`.

## Compilação

### Build upstream (sem ponte)

Sem features ativas, o fork é um superconjunto estrito do upstream; as instruções upstream se aplicam sem mudanças. Dependências e comandos completos em [`../README.md`](../README.md).

### Build com ponte (Windows MSVC, recomendado)

A ponte só suporta Windows no momento (transporte named pipe e agente VHDMount).

Ambiente exigido:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <caminho para LLVM\x64\bin>
```

Em seguida, preencha `secret.sec` (dev-only) ou defina as variáveis equivalentes, então:

```sh
# Build sidecar de produção (ponte ON, controlador removido)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Apenas ponte (UI do controlador mantida para iteração de dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verificação

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Última execução neste branch: 0 erros / 189 unit / 6 + 8 integração / 38 + 4 build_support.

## Segredos e CI

A ponte exige cinco entradas em build time:

| Variável | Papel | Formato |
|---|---|---|
| `HBBS_KEY` | Chave pública do rendezvous server (sobrescreve `RS_PUB_KEY`) | base64, 32 bytes após decode |
| `HBBS_HOST` | Host do rendezvous server | `host[:port[-port2]]` |
| `HBBR_HOST` | Host do relay server | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (ou `_B64`) | Segredo compartilhado HMAC de 32 bytes | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Versão monotônica de rotação de chave | inteiro não negativo |

Dois caminhos:

1. **Dev local** &mdash; preencher `secret.sec` na raiz com `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Ignorado por [`.gitignore`](../.gitignore).
2. **CI** &mdash; configurar os mesmos nomes como repository secrets do GitHub Actions; [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) os injeta como variáveis mascaradas. **`secret.sec` nunca é materializado nos runners**.

`secret.sec` e `vhd_bridge_secret.bin` estão em `.gitignore` e **não devem ser commitados**. `scripts/check_bridge_strings.ps1` é a rede de segurança pós-build.

## Licença e atribuição

Este fork é distribuído sob a mesma licença que o upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Texto completo em [`../LICENCE`](../LICENCE); este fork **não modifica** a licença.

- Todos os direitos autorais sobre o código upstream do RustDesk pertencem aos autores e contribuidores upstream, ver <https://github.com/rustdesk/rustdesk>.
- As modificações deste fork (features `vhd-bridge` / `controlled-only` e código de suporte) também são distribuídas sob AGPL-3.0; usuários downstream retêm todos os direitos concedidos pela AGPL-3.0, incluindo direito ao código fonte correspondente em qualquer implantação em rede.
- O nome e o logotipo "RustDesk" pertencem ao projeto upstream; este fork os usa apenas para identificar a base de código modificada, em conformidade com o uso justo de marcas em forks de software livre.
- Bibliotecas de terceiros (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; SDK Sciter; dependências Flutter) mantêm suas licenças originais.

Usar este fork implica aceitar a AGPL-3.0 e o **aviso sobre uso indevido** no topo deste arquivo.
