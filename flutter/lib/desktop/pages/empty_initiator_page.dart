// flutter/lib/desktop/pages/empty_initiator_page.dart
//
// vhd-machine-auth-bridge §17.6 / Requirement 20.8:
//
// Placeholder widgets used in the `RustDesk_Controlled` (controlled-
// only) build form to replace initiator entry points (`ConnectionPage`,
// `RemotePage`, `FileTransferPage`, `PortForwardPage`,
// `ViewCameraPage`, `TerminalPage`).
//
// The Dart AOT compiler tree-shakes the real initiator pages out of
// the product whenever `kControlledOnly` is true at build time, so
// initiator-only i18n keys ("Connect", "Recent sessions",
// "Address Book", ...) never reach the binary. The classes here
// reference no such strings.

import 'package:flutter/material.dart';

/// Drop-in replacement for `ConnectionPage` on the home page right
/// pane. Renders nothing visible (so the `MaintenanceOverlay` and the
/// left pane retain full control of the screen real estate) without
/// pulling in any peer-tab / connect-bar i18n strings.
class EmptyInitiatorPage extends StatelessWidget {
  const EmptyInitiatorPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(color: Theme.of(context).scaffoldBackgroundColor);
  }
}
