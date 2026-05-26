<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> con il bridge di autenticazione macchina VHD</b><br>
  <a href="#stato-del-fork">Stato del fork</a> &bull;
  <a href="#cosa-aggiunge-questo-fork">Aggiunte</a> &bull;
  <a href="#compilazione">Compilazione</a> &bull;
  <a href="#segreti-e-ci">Segreti &amp; CI</a> &bull;
  <a href="#licenza-e-attribuzione">Licenza</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Questo repository è un fork derivato da [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Documentazione inglese completa: [`../README.md`](../README.md).
> I diritti d'autore, i marchi e la licenza AGPL-3.0 a monte restano invariati &mdash; vedi [Licenza e attribuzione](#licenza-e-attribuzione).

> [!Caution]
> **Avviso sull'uso improprio:** gli sviluppatori upstream di RustDesk e i manutentori di questo fork non tollerano né supportano alcun uso non etico o illegale di questo software. L'uso improprio &mdash; accesso, controllo o violazione della privacy non autorizzati &mdash; è severamente vietato. Gli autori non sono responsabili di alcun uso improprio.

---

## Stato del fork

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (configurato come remote `upstream` in git) |
| **Questo fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Branch attivo** | `feature/vhd-machine-auth-bridge` |
| **Submodule** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), stesso branch |
| **Licenza** | AGPL-3.0 (invariata rispetto a upstream &mdash; vedi [`LICENCE`](../LICENCE)) |
| **Obiettivo** | Far girare il lato Controlled di RustDesk come sidecar dell'agente VHDMount esterno tramite un bridge autenticato e vincolato alla macchina. |

Quando `vhd-bridge` è **disattivato**, l'artefatto compilato è equivalente comportamentalmente a RustDesk upstream &mdash; invariante verificato automaticamente da `tests/feature_off_parity.rs`.

## Cosa aggiunge questo fork

Un sottosistema coeso &mdash; il **bridge di autenticazione macchina VHD** &mdash; controllato da due Cargo features, **disattivate di default**:

- **`vhd-bridge`** &mdash; compila il worker del bridge, il cablaggio IPC, l'overlay UI di manutenzione e gli smoke test.
- **`controlled-only`** &mdash; rimuove UI e percorsi del Controller (initiator) per produrre un binario che può essere solo controllato; da combinare con `vhd-bridge` per la build sidecar di produzione.

Senza feature attive, `cargo run` e il flusso upstream funzionano identici.

### Modifiche principali

- **`src/vhd_bridge/`** &ndash; worker named pipe, macchina a stati `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 con segreto condiviso a 32 byte iniettato a build time, backoff di riconnessione, osservabilità strutturata, log sink che redige i segreti.
- **`src/server/connection.rs`** &ndash; gate di approvazione: prima di accettare un peer in ingresso, consulta il peer-set di autenticazione macchina del bridge.
- **`src/auth_2fa.rs`** &ndash; 2FA forzato a OFF mentre il bridge governa l'autenticazione (verificato da `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay che riflette lo stato del bridge (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate di supporto condivisa tra `build.rs` e CI: gate di prerequisiti rigoroso, parser tollerante per `secret.sec`, test di coerenza con la documentazione del protocollo.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; riferimento del wire protocol.
- **`scripts/check_bridge_strings.ps1`** &ndash; scanner post-build che assicura che nessun byte in chiaro di `HBBS Key` / `VHDMount Key` finisca negli artefatti.
- **`.github/workflows/build.yml`** &mdash; workflow CI multipiattaforma; i job Windows chiave sono **controller-windows** (bundle Flutter desktop, feature di default + `hwcodec` + `vram` + `flutter`, senza bridge) e **controlled-windows** (sidecar controlled, `--features vhd-bridge,controlled-only,hwcodec,vram`), con esecuzione degli script di leak e smoke.

Specifica completa in [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Clonazione

Il fork modifica l'URL del submodule `libs/hbb_common`, quindi clona ricorsivamente:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Se hai già clonato col `.gitmodules` upstream: `git submodule sync && git submodule update --init --recursive`.

## Compilazione

### Build upstream (senza bridge)

Senza feature attive, questo fork è un superinsieme stretto di upstream; le istruzioni upstream si applicano invariate. Dipendenze e comandi completi in [`../README.md`](../README.md).

### Build col bridge (Windows MSVC, consigliato)

Il bridge supporta solo Windows al momento (trasporto named pipe e agente VHDMount).

Ambiente richiesto:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <percorso a LLVM\x64\bin>
```

Poi popola `secret.sec` (solo dev) o imposta le variabili d'ambiente corrispondenti, infine:

```sh
# Build sidecar di produzione (bridge ON, controller rimosso)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# Solo bridge (UI controller mantenuta per dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Verifica

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Ultima esecuzione su questo branch: 0 errori / 189 unit / 6 + 8 integration / 38 + 4 build_support.

## Segreti e CI

Il bridge richiede cinque input a build time:

| Variabile | Scopo | Formato |
|---|---|---|
| `HBBS_KEY` | Chiave pubblica del rendezvous server (sovrascrive `RS_PUB_KEY`) | base64, 32 byte decodificati |
| `HBBS_HOST` | Host del rendezvous server | `host[:port[-port2]]` |
| `HBBR_HOST` | Host del relay server | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (o `_B64`) | Segreto condiviso HMAC a 32 byte | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Versione di rotazione monotona | intero non negativo |

Due percorsi:

1. **Dev locale** &mdash; popola `secret.sec` nella radice con `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Il file è ignorato da [`.gitignore`](../.gitignore).
2. **CI** &mdash; configura gli stessi nomi come repository secret di GitHub Actions; [`.github/workflows/build.yml`](../.github/workflows/build.yml) li inietta come variabili d'ambiente mascherate. **`secret.sec` non viene mai materializzato sui runner**.

`secret.sec` e `vhd_bridge_secret.bin` sono entrambi in `.gitignore` e **non vanno mai committati**. `scripts/check_bridge_strings.ps1` è la rete di sicurezza post-build.

## Licenza e attribuzione

Questo fork è distribuito con la stessa licenza di upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Testo completo in [`../LICENCE`](../LICENCE); questo fork **non la modifica**.

- Tutti i diritti d'autore sul codice RustDesk upstream restano agli autori e contributori upstream, vedi <https://github.com/rustdesk/rustdesk>.
- Le modifiche di questo fork (feature `vhd-bridge` e `controlled-only` e codice di supporto) sono anch'esse distribuite con AGPL-3.0; gli utenti downstream mantengono tutti i diritti concessi dalla AGPL-3.0, incluso il diritto al codice sorgente corrispondente per qualsiasi distribuzione di rete.
- Il nome e il logo "RustDesk" appartengono al progetto upstream; questo fork li usa unicamente per identificare la base di codice modificata, secondo il fair use dei marchi nei fork di software libero.
- Le librerie di terze parti (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`; SDK Sciter; dipendenze Flutter) mantengono le licenze originali.

L'uso di questo fork comporta l'accettazione dell'AGPL-3.0 e dell'**avviso sull'uso improprio** in cima al file.
