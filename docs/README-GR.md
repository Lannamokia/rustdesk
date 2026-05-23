<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; fork <code>Lannamokia</code> με τη γέφυρα ταυτοποίησης μηχανής VHD</b><br>
  <a href="#κατάσταση-fork">Κατάσταση fork</a> &bull;
  <a href="#τι-προσθέτει-το-fork">Προσθήκες</a> &bull;
  <a href="#μεταγλώττιση">Μεταγλώττιση</a> &bull;
  <a href="#μυστικά-και-ci">Μυστικά &amp; CI</a> &bull;
  <a href="#άδεια-και-απόδοση">Άδεια</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-ZH.md">中文</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> Αυτό το αποθετήριο είναι downstream fork του [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk). Πλήρης αγγλική τεκμηρίωση: [`../README.md`](../README.md).
> Πνευματικά δικαιώματα, εμπορικά σήματα και η άδεια AGPL-3.0 του upstream παραμένουν αμετάβλητα &mdash; βλ. [Άδεια και απόδοση](#άδεια-και-απόδοση).

> [!Caution]
> **Αποποίηση κατάχρησης:** οι upstream προγραμματιστές του RustDesk και οι συντηρητές αυτού του fork δεν ανέχονται και δεν υποστηρίζουν καμία ανήθικη ή παράνομη χρήση του λογισμικού. Μη εξουσιοδοτημένη πρόσβαση, έλεγχος ή παραβίαση ιδιωτικότητας απαγορεύονται αυστηρά. Οι συγγραφείς δεν ευθύνονται για τυχόν κατάχρηση.

---

## Κατάσταση fork

| | |
|---|---|
| **Upstream** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) (στο git ως remote `upstream`) |
| **Αυτό το fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **Ενεργός κλάδος** | `feature/vhd-machine-auth-bridge` |
| **Υπομονάδα** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common), ίδιος κλάδος |
| **Άδεια** | AGPL-3.0 (αμετάβλητη σε σχέση με upstream &mdash; βλ. [`LICENCE`](../LICENCE)) |
| **Στόχος** | Εκτέλεση της ελεγχόμενης πλευράς του RustDesk ως sidecar του εξωτερικού πράκτορα VHDMount πάνω από μια ταυτοποιημένη και δεμένη με τη μηχανή γέφυρα. |

Όταν το `vhd-bridge` είναι **απενεργοποιημένο**, το παραγόμενο εκτελέσιμο συμπεριφέρεται όπως το upstream RustDesk &mdash; η αναλλοίωτη επαληθεύεται αυτόματα από το `tests/feature_off_parity.rs`.

## Τι προσθέτει το fork

Ένα συνεκτικό υποσύστημα &mdash; **η γέφυρα ταυτοποίησης μηχανής VHD** &mdash; ελεγχόμενο από δύο Cargo features, **απενεργοποιημένα από προεπιλογή**:

- **`vhd-bridge`** &mdash; συμπεριλαμβάνει στο build τον worker της γέφυρας, την IPC καλωδίωση, το overlay UI συντήρησης και τα smoke tests.
- **`controlled-only`** &mdash; αφαιρεί το UI και τους κώδικες του Controller (initiator), παράγοντας ένα μόνο-ελεγχόμενο binary· συνδυάζεται με το `vhd-bridge` για το παραγωγικό sidecar build.

Χωρίς ενεργές features, το `cargo run` και η ροή του upstream build λειτουργούν αμετάβλητα.

### Κύριες αλλαγές

- **`src/vhd_bridge/`** &ndash; worker named pipe, μηχανή καταστάσεων `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`, HMAC-SHA256 με 32-byte κοινό μυστικό που εγχέεται κατά το build, backoff επανασύνδεσης, δομημένη παρατηρησιμότητα, log sink που αποκρύπτει μυστικά.
- **`src/server/connection.rs`** &ndash; πύλη έγκρισης: πριν την αποδοχή εισερχόμενου peer, συμβουλεύεται το peer-set ταυτοποίησης μηχανής που διατηρεί η γέφυρα.
- **`src/auth_2fa.rs`** &ndash; το 2FA επιβάλλεται OFF όσο η γέφυρα κυβερνά την ταυτοποίηση (επαληθεύεται από το `tests/smoke_2fa_disabled.rs`).
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; overlay που αντικατοπτρίζει την κατάσταση της γέφυρας (`active / starting / lost`).
- **`libs/build_support/`** &ndash; βοηθητικό crate κοινό για `build.rs` και CI: αυστηρή πύλη προαπαιτούμενων, ανεκτικός parser για `secret.sec`, έλεγχος συνέπειας με τη τεκμηρίωση πρωτοκόλλου.
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; αναφορά πρωτοκόλλου σύρματος.
- **`scripts/check_bridge_strings.ps1`** &ndash; σαρωτής διαρροών μετά το build: εγγυάται ότι κανένα plaintext byte από `HBBS Key` / `VHDMount Key` δεν διαρρέει στα παράγωγα.
- **`.github/workflows/vhd-bridge.yml`** &mdash; μήτρα CI που χτίζει τα Windows artifacts feature-on / feature-off / controlled-only.

Πλήρης προδιαγραφή στο [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge).

## Κλωνοποίηση

Το fork αλλάζει το URL της υπομονάδας `libs/hbb_common`· κλωνοποιήστε αναδρομικά:

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

Αν είχατε κλωνοποιήσει με το upstream `.gitmodules`: `git submodule sync && git submodule update --init --recursive`.

## Μεταγλώττιση

### Upstream build (χωρίς γέφυρα)

Χωρίς ενεργά features, το fork είναι αυστηρό υπερσύνολο του upstream· οι οδηγίες upstream ισχύουν αμετάβλητες. Πλήρεις εξαρτήσεις και εντολές στο [`../README.md`](../README.md).

### Build με γέφυρα (Windows MSVC, συνιστάται)

Η γέφυρα προς το παρόν υποστηρίζει μόνο Windows (μεταφορά named pipe και πράκτορας VHDMount).

Απαιτούμενο περιβάλλον:

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <διαδρομή προς LLVM\x64\bin>
```

Στη συνέχεια συμπληρώστε το dev-only `secret.sec` ή ορίστε τις αντίστοιχες env μεταβλητές, και:

```sh
# Παραγωγικό sidecar build (γέφυρα ON, controller αφαιρεμένος)
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# Μόνο γέφυρα (UI controller διατηρείται για dev)
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### Επαλήθευση

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

Τελευταία εκτέλεση σε αυτόν τον κλάδο: 0 σφάλματα / 189 unit / 6 + 8 ολοκλήρωσης / 38 + 4 build_support.

## Μυστικά και CI

Η γέφυρα απαιτεί πέντε εισόδους κατά το build:

| Μεταβλητή | Σκοπός | Μορφή |
|---|---|---|
| `HBBS_KEY` | Δημόσιο κλειδί διακομιστή rendezvous (επικαλύπτει το `RS_PUB_KEY`) | base64, 32 bytes μετά την αποκωδικοποίηση |
| `HBBS_HOST` | Host του διακομιστή rendezvous | `host[:port[-port2]]` |
| `HBBR_HOST` | Host του διακομιστή relay | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX` (ή `_B64`) | Κοινό μυστικό HMAC 32 bytes | 64 hex / 44 base64 |
| `VHD_BRIDGE_SECRET_VERSION` | Μονοτονική έκδοση εναλλαγής κλειδιού | μη αρνητικός ακέραιος |

Δύο διαδρομές:

1. **Τοπική ανάπτυξη** &mdash; συμπληρώστε το `secret.sec` στη ρίζα του αποθετηρίου με γραμμές `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`. Το αρχείο αγνοείται από το [`.gitignore`](../.gitignore).
2. **CI** &mdash; ρυθμίστε τα ίδια ονόματα ως repository secrets στο GitHub Actions· το [`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) τα εγχέει ως καλυμμένες env μεταβλητές. **Το `secret.sec` δεν υλοποιείται ποτέ στους runners**.

`secret.sec` και `vhd_bridge_secret.bin` βρίσκονται και τα δύο στο `.gitignore` και **δεν πρέπει ποτέ να γίνουν commit**. Το `scripts/check_bridge_strings.ps1` είναι το δίχτυ ασφαλείας μετά το build.

## Άδεια και απόδοση

Το fork διανέμεται με την ίδια άδεια με το upstream: **GNU Affero General Public License v3.0 (AGPL-3.0)**. Πλήρες κείμενο στο [`../LICENCE`](../LICENCE)· το fork **δεν τροποποιεί** την άδεια.

- Όλα τα πνευματικά δικαιώματα στον κώδικα του upstream RustDesk παραμένουν στους upstream συγγραφείς και συνεισφέροντες, βλ. <https://github.com/rustdesk/rustdesk>.
- Οι τροποποιήσεις που εισάγει αυτό το fork (features `vhd-bridge` / `controlled-only` και ο υποστηρικτικός κώδικας) διανέμονται επίσης υπό AGPL-3.0· οι downstream χρήστες διατηρούν όλα τα δικαιώματα που παραχωρεί η AGPL-3.0, συμπεριλαμβανομένου του δικαιώματος στον αντίστοιχο πηγαίο κώδικα για κάθε δικτυακή ανάπτυξη.
- Το όνομα και το λογότυπο "RustDesk" ανήκουν στο upstream έργο· το fork τα χρησιμοποιεί μόνο για να αναγνωρίσει την τροποποιημένη βάση κώδικα, σύμφωνα με τη δίκαιη χρήση εμπορικών σημάτων σε forks ελεύθερου λογισμικού.
- Βιβλιοθήκες τρίτων (vcpkg: `libvpx`, `libyuv`, `opus`, `aom`· Sciter SDK· εξαρτήσεις Flutter) διατηρούν τις αρχικές τους άδειες.

Η χρήση αυτού του fork συνιστά αποδοχή της AGPL-3.0 και της **αποποίησης κατάχρησης** στην αρχή του αρχείου.
