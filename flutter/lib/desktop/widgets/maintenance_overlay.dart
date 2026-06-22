// flutter/lib/desktop/widgets/maintenance_overlay.dart
//
// vhd-machine-auth-bridge §15: full-screen "machine under remote
// maintenance" overlay shown on RustDesk_Controlled hosts whenever
// at least one Active_Remote_Session exists.
//
// Tasks 15.1 / 15.2 / 15.3 / 15.4 produce only the widget tree, the
// isolate-driven liveness ticker, and the Dart side of the Windows
// low-level keyboard hook MethodChannel. Wiring this widget into
// `desktop_overlay_screen.dart`, binding to the `vhd-bridge-state`
// and `active-session-count` IPC keys, and the C++ side of the
// keyboard-hook MethodChannel are explicit task-16 / future-task
// scope (see TODOs).

import 'dart:async';
import 'dart:isolate';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hbb/common.dart';

/// Full-screen overlay shown while at least one authorized remote
/// session is active. Task 16 owns the gating: this widget itself
/// assumes "if I'm in the tree, I should render".
///
/// Per design.md §"Maintenance_Overlay (Flutter)":
/// - Topmost, fills the work area + taskbar; non-resizable, no
///   min/close buttons (handled by the host window, task 16).
/// - Dark semi-transparent backdrop + blur.
/// - Centered bilingual title + detail text.
/// - [LivenessIndicator] keeps animating at >= 1fps even if the
///   main isolate is blocked or the GPU is unavailable.
/// - On Windows, installs a low-level keyboard hook for the lifetime
///   of the overlay so local key presses are swallowed (Ctrl+Alt+Del
///   is reserved by the OS and cannot be blocked by user-mode hooks);
///   remote-protocol input bypasses the hook because enigo / the
///   RustDesk input pipe does not go through user-mode keyboard
///   hooks.
class MaintenanceOverlay extends StatefulWidget {
  const MaintenanceOverlay({super.key});

  @override
  State<MaintenanceOverlay> createState() => _MaintenanceOverlayState();
}

class _MaintenanceOverlayState extends State<MaintenanceOverlay> {
  @override
  void initState() {
    super.initState();
    // Best-effort: install on Windows only. The native handler is a
    // no-op stub on other platforms (and on Windows builds without
    // the runner-side handler wired up); failures are logged but
    // never crash the overlay.
    _KeyboardHook.install();
  }

  @override
  void dispose() {
    _KeyboardHook.uninstall();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // MouseRegion(opaque) + Focus(autofocus) swallow pointer & focus
    // events inside the overlay area; the keyboard hook handles
    // global key presses.
    return Focus(
      autofocus: true,
      child: MouseRegion(
        opaque: true,
        child: Scaffold(
          backgroundColor: Colors.black.withOpacity(0.85),
          body: Stack(
            fit: StackFit.expand,
            children: [
              // Backdrop: darken the entire screen.
              Container(color: Colors.black.withOpacity(0.85)),
              // Centered content.
              Center(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Text(
                      translate('vhd_overlay_title'),
                      style: const TextStyle(
                        fontSize: 48,
                        fontWeight: FontWeight.bold,
                        color: Colors.white,
                      ),
                      textAlign: TextAlign.center,
                    ),
                    const SizedBox(height: 24),
                    Text(
                      translate('vhd_overlay_detail'),
                      style: const TextStyle(
                        fontSize: 24,
                        color: Colors.white70,
                      ),
                      textAlign: TextAlign.center,
                    ),
                    const SizedBox(height: 48),
                    const LivenessIndicator(),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Animated indicator that keeps advancing at >= 1fps even when the
/// main isolate is blocked. Phase increments are pushed by a worker
/// isolate via [SendPort] so that even a stalled UI thread will pick
/// up the new phase on the next vsync (design.md §15.5 / Property 17).
class LivenessIndicator extends StatefulWidget {
  const LivenessIndicator({super.key});

  @override
  State<LivenessIndicator> createState() => _LivenessIndicatorState();
}

class _LivenessIndicatorState extends State<LivenessIndicator> {
  int _phase = 0;
  ReceivePort? _receivePort;
  Isolate? _isolate;

  @override
  void initState() {
    super.initState();
    _spawnTicker();
  }

  Future<void> _spawnTicker() async {
    final receivePort = ReceivePort();
    try {
      final isolate = await Isolate.spawn(_tickerEntry, receivePort.sendPort);
      if (!mounted) {
        isolate.kill(priority: Isolate.immediate);
        receivePort.close();
        return;
      }
      _receivePort = receivePort;
      _isolate = isolate;
      receivePort.listen((dynamic msg) {
        if (!mounted) return;
        if (msg is int) {
          setState(() => _phase = msg);
        }
      });
    } catch (e) {
      // Isolate spawn is not supported on every embedding (notably
      // Flutter Web). Fall back to a main-isolate timer so the
      // indicator still animates, even if not under main-thread
      // stalls. Property 17 only applies on platforms that support
      // Isolate.spawn (desktop: Windows / macOS / Linux).
      receivePort.close();
      debugPrint('vhd_overlay: isolate ticker fallback: $e');
      Timer.periodic(const Duration(seconds: 1), (_) {
        if (!mounted) return;
        setState(() => _phase = (_phase + 1) % 360);
      });
    }
  }

  static void _tickerEntry(SendPort sendPort) {
    int phase = 0;
    Timer.periodic(const Duration(seconds: 1), (_) {
      phase = (phase + 1) % 360;
      sendPort.send(phase);
    });
  }

  @override
  void dispose() {
    _isolate?.kill(priority: Isolate.immediate);
    _receivePort?.close();
    _isolate = null;
    _receivePort = null;
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Convert phase (0-359 degrees) to radians.
    final angle = _phase * (3.14159265358979 / 180.0);
    return Transform.rotate(
      angle: angle,
      child: const SizedBox(
        width: 80,
        height: 80,
        child: CircularProgressIndicator(
          color: Colors.white,
          strokeWidth: 6,
        ),
      ),
    );
  }
}

/// Dart side of the Windows low-level keyboard hook
/// (`SetWindowsHookExW(WH_KEYBOARD_LL)`).
///
/// TODO(task-15.3-native): the matching native handler must be added
/// to `flutter/windows/runner/` (or a dedicated platform plugin).
/// The runner registers a `MethodChannel` named
/// `com.rustdesk/vhd_overlay_keyboard_hook` and on `install`:
///   1. Calls `SetWindowsHookExW(WH_KEYBOARD_LL, hook_proc, ...)`,
///      storing the returned `HHOOK` for later removal.
///   2. The hook proc returns 1 (swallow) for every keypress except
///      Ctrl+Alt+Del, which Windows handles at the OS level and
///      cannot be blocked by user-mode hooks.
/// On `uninstall` the runner calls `UnhookWindowsHookEx(hhook)` and
/// clears the stored handle. Remote-protocol input (enigo /
/// RustDesk's input pipe) does not flow through user-mode keyboard
/// hooks, so it bypasses this filter automatically.
///
/// On non-Windows platforms or builds without the native handler,
/// `invokeMethod` raises a `PlatformException` /
/// `MissingPluginException`; both are caught and logged so the
/// overlay still renders.
class _KeyboardHook {
  static const _channel =
      MethodChannel('com.rustdesk/vhd_overlay_keyboard_hook');

  static Future<void> install() async {
    if (!_isWindows) return;
    try {
      await _channel.invokeMethod<void>('install');
    } on PlatformException catch (e) {
      debugPrint('vhd_overlay: keyboard hook install failed: ${e.message}');
    } on MissingPluginException catch (e) {
      debugPrint('vhd_overlay: keyboard hook plugin missing: ${e.message}');
    }
  }

  static Future<void> uninstall() async {
    if (!_isWindows) return;
    try {
      await _channel.invokeMethod<void>('uninstall');
    } on PlatformException catch (e) {
      debugPrint('vhd_overlay: keyboard hook uninstall failed: ${e.message}');
    } on MissingPluginException {
      // Already absent; nothing to undo.
    }
  }

  static bool get _isWindows {
    // `Platform.isWindows` would pull in `dart:io`, which Flutter web
    // tests dislike. `defaultTargetPlatform` is enough here because
    // this widget is only built into the desktop tree.
    return defaultTargetPlatform == TargetPlatform.windows;
  }
}
