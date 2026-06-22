<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> avec le bridge d'authentification machine VHD</b><br>
  <a href="#état-du-fork">État du fork</a> &bull;
  <a href="#ce-que-ce-fork-ajoute">Ajouts</a> &bull;
  <a href="#compilation">Compilation</a> &bull;
  <a href="#secrets-et-ci">Secrets &amp; CI</a> &bull;
  <a href="#licence-et-attribution">Licence</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Ce dépôt est un fork en aval de [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Documentation anglaise complète : [`../README.md`](../README.md).
> Les droits d'auteur, marques et la licence AGPL-3.0 de l'amont restent inchangés &mdash; voir [Licence et attribution](#licence-et-attribution).

> [!Caution]
> **Avertissement d'usage abusif :** les développeurs amont de RustDesk et les mainteneurs de ce fork ne tolèrent ni n'accompagnent aucun usage non éthique ou illégal de ce logiciel. L'usage abusif &mdash; accès, contrôle ou intrusion non autorisés &mdash; est strictement contraire à nos directives. Les auteurs déclinent toute responsabilité en cas d'usage abusif.

---

## État du fork

| | |
|---|---|
| **Amont** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (configuré comme remote `upstream`) |
| **Ce fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Branche active** | `feature/vhd-machine-auth-bridge` |
| **Sous-module** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), même branche |
| **Licence** | AGPL-3.0 (identique à l'amont &mdash; voir [`LICENCE`](../LICENCE)) |
| **Objectif** | Faire fonctionner le côté contrôlé de RustDesk en sidecar de l'agent VHDMount externe, via un bridge authentifié et lié à la machine. |

Quand `vhd-bridge` est **désactivé**, l'artefact compilé se comporte exactement comme RustDesk amont &mdash; cet invariant est vérifié automatiquement par `tests/feature_off_parity.rs`.

## Ce que ce fork ajoute

Un sous-système cohérent &mdash; le **bridge d'authentification machine VHD** &mdash; piloté par deux features Cargo, **désactivées par défaut** :

- **`vhd-bridge`** &mdash; embarque le worker du bridge, le câblage IPC, l'overlay UI de maintenance et les tests smoke.
- **`controlled-only`** &mdash; supprime l'UI Contrôleur (initiator) et ses chemins de code pour produire une binary uniquement contrôlable ; à combiner avec `vhd-bridge` pour la build sidecar de production.

Sans aucune feature active, `cargo run` et le flux de build amont fonctionnent à l'identique.

### Surfaces principales

- **`src/vhd_bridge/`** &ndash; worker named-pipe, machine à états `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 avec un secret partagé de 32 octets injecté à la compilation, backoff de reconnexion, observabilité structurée, log sink occultant les secrets.
- **`src/server/connection.rs`** &ndash; passerelle d'approbation : avant d'accepter un pair entrant, on consulte le peer-set d'authentification machine maintenu par le bridge.
- **`src/auth_2fa.rs`** &ndash; 2FA forcée à OFF tant que le bridge gouverne l'authentification (vérifié par `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay reflétant l'état du bridge (`active / starting / lost`).
- **`libs/build_support/`** &ndash; crate utilitaire partagée par `build.rs` et la CI : porte de prérequis stricte, parseur tolérant pour `secret.sec`, test de cohérence avec la doc protocole.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; référence du protocole de fil.
- **`scripts/check_bridge_strings.ps1`** &ndash; scanner post-build s'assurant qu'aucun octet en clair de `HBBS Key` / `VHDMount Key` ne fuit dans les artefacts.
- **`.github/workflows/build.yml`** &mdash; workflow CI multiplateforme ; les jobs Windows clés sont **controller-windows** (bundle Flutter desktop, features par défaut + `hwcodec` + `vram` + `flutter`, sans bridge) et **controlled-windows** (sidecar côté contrôlé, `--features vhd-bridge,controlled-only,hwcodec,vram`), avec exécution des scripts de fuite + smoke.

Spécification complète : [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Cloner

Le fork modifie l'URL du sous-module `libs/hbb_common`, d'où le clone récursif :

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Si tu as déjà cloné avec le `.gitmodules` amont : `git submodule sync && git submodule update --init --recursive`.

## Compilation

### Build amont (sans bridge)

Sans feature active, ce fork est un sur-ensemble strict de l'amont ; les instructions amont s'appliquent telles quelles. Dépendances et commandes complètes : [`../README.md`](../README.md).

### Build avec bridge (Windows MSVC, recommandé)

Le bridge ne supporte actuellement que Windows (transport named-pipe et agent VHDMount).

Environnement requis :

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <chemin vers LLVM\x64\bin>
```

Ensuite, soit remplir `secret.sec` (dev-only), soit définir les variables d'environnement correspondantes, puis :

```sh
# Build sidecar production (bridge ON, contrôleur retiré)
cargo build --release --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc

# Bridge seul (UI contrôleur conservée pour le dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Vérification

```sh
cargo check --lib --features vhd-bridge,controlled-only,hwcodec,vram --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only,hwcodec,vram
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Dernière exécution sur cette branche : 0 erreur / 189 tests unitaires / 6 + 8 tests d'intégration / 38 + 4 tests build_support.

## Secrets et CI

Le bridge exige cinq entrées au moment de la compilation :

| Variable | Rôle | Format |
|---|---|---|
| `HBBS_KEY` | Clé publique du serveur de rendezvous (remplace `RS_PUB_KEY`) | base64, 32 octets après décodage |
| `HBBS_HOST` | Hôte du serveur de rendezvous | `host[:port[-port2]]` |
| `HBBR_HOST` | Hôte du serveur de relais | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (ou `_B64`) | Secret partagé HMAC de 32 octets | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Version de rotation de clé monotone croissante | entier non négatif |

Deux voies :

1. **Dev local** &mdash; remplir `secret.sec` à la racine avec `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Le fichier est ignoré par [`.gitignore`](../.gitignore).
2. **CI** &mdash; déclarer les mêmes noms en tant que secrets de dépôt GitHub Actions. [`.github/workflows/build.yml`](../.github/workflows/build.yml) les injecte via des variables d'environnement masquées ; **`secret.sec` n'est jamais matérialisé sur les runners**.

`secret.sec` et `vhd_bridge_secret.bin` figurent tous deux dans `.gitignore` et **ne doivent jamais être committés**. `scripts/check_bridge_strings.ps1` est le filet de sécurité post-build.

## Licence et attribution

Ce fork est distribué sous la même licence que l'amont : **GNU Affero General Public License v3.0 (AGPL-3.0)**. Texte complet : [`../LICENCE`](../LICENCE) ; ce fork **ne le modifie pas**.

- Tous les droits d'auteur sur la base de code RustDesk amont demeurent ceux des auteurs et contributeurs amont, voir <https://github.com/rustdesk/rustdesk>.
- Les modifications introduites par ce fork (features `vhd-bridge` et `controlled-only` et code associé) sont également distribuées sous AGPL-3.0 ; les utilisateurs en aval conservent l'ensemble des droits accordés par l'AGPL-3.0, dont la fourniture du code source correspondant pour tout déploiement réseau.
- Le nom et le logo "RustDesk" appartiennent au projet amont ; ce fork ne les utilise que pour identifier la base de code modifiée, conformément à l'usage équitable des marques pour les forks de logiciels libres.
- Les bibliothèques tierces (vcpkg : `libvpx`, `libyuv`, `opus`, `aom` ; Sciter SDK ; dépendances Flutter) conservent leurs licences d'origine.

L'utilisation de ce fork vaut acceptation de l'AGPL-3.0 et de l'**avertissement d'usage abusif** en tête de fichier.
