// flutter/test/liveness_indicator_test.dart
//
// vhd-machine-auth-bridge §15.6 — Property 17:
//   "Liveness_Indicator >= 1 fps under main-thread stalls".
//
// The production widget [LivenessIndicator] (see
// flutter/lib/desktop/widgets/maintenance_overlay.dart) drives its
// rotating phase counter from a side-channel `Isolate.spawn`'d
// worker that runs `Timer.periodic(Duration(seconds: 1))`. The whole
// point of that design is to advance the phase even when the main
// isolate is blocked — otherwise a stalled UI thread would render
// "machine under remote maintenance" indistinguishable from "machine
// frozen" to a bystander (Requirement 15.5).
//
// This file verifies the **isolate contract** that the widget relies
// on: ticks fire on wall-clock time inside the worker isolate, are
// queued by the main isolate's [ReceivePort], and drain in monotone
// +1-mod-360 order once the main isolate is unblocked. Property 16 /
// 18 (overlay visibility, input blocking) live in 15.5 and own
// `flutter/test/maintenance_overlay_test.dart`.
//
// Validates: Requirements 15.5
@TestOn('vm')
library liveness_indicator_test;

import 'dart:async';
import 'dart:isolate';

// `glados.dart` re-exports `package:test/test.dart` (group / test /
// expect / matchers). Importing `package:flutter_test/...` here as
// well would collide on every one of those symbols; this file only
// exercises the isolate contract behind `LivenessIndicator`, no
// widget tree, so plain `test` is enough.
import 'package:glados/glados.dart';

/// Mirror of `_LivenessIndicatorState._tickerEntry` in
/// flutter/lib/desktop/widgets/maintenance_overlay.dart. Kept in
/// sync by hand: if the production entrypoint changes (cadence,
/// payload type, modulus), update this mirror too. The test isolate
/// contract is intentionally independent from the widget tree so
/// that a stalled main isolate cannot influence what the test
/// observes.
void _tickerEntry(SendPort sendPort) {
  var phase = 0;
  Timer.periodic(const Duration(seconds: 1), (_) {
    phase = (phase + 1) % 360;
    sendPort.send(phase);
  });
}

/// Minimal harness that consumes ticks from a worker isolate the
/// same way the widget does (a [ReceivePort] subscription).
class _LivenessProbe {
  ReceivePort? _port;
  Isolate? _isolate;
  StreamSubscription<dynamic>? _sub;
  final List<int> phases = <int>[];

  Future<void> start() async {
    final port = ReceivePort();
    _isolate = await Isolate.spawn(_tickerEntry, port.sendPort);
    _port = port;
    _sub = port.listen((dynamic msg) {
      if (msg is int) phases.add(msg);
    });
  }

  Future<void> stop() async {
    _isolate?.kill(priority: Isolate.immediate);
    await _sub?.cancel();
    _port?.close();
    _isolate = null;
    _port = null;
    _sub = null;
  }
}

/// Block the main isolate event loop for [millis] ms via a busy
/// wait. We deliberately avoid `await Future.delayed`: that yields
/// to the event loop and lets queued [ReceivePort] messages drain,
/// which would defeat the whole point of Property 17. The
/// arithmetic body keeps the loop observable so the VM cannot
/// elide it.
void _stallMainIsolate(int millis) {
  if (millis <= 0) return;
  final sw = Stopwatch()..start();
  var sink = 0;
  while (sw.elapsedMilliseconds < millis) {
    sink ^= sw.elapsedMicroseconds;
  }
  // Statistically improbable; keeps `sink` live to the optimizer.
  if (sink == 0xDEADBEEF) {
    // ignore: avoid_print
    print('improbable sink value reached');
  }
}

void main() {
  group('LivenessIndicator (Property 17)', () {
    // Property 17, glados-driven. Δt ∈ [0, 5000] ms. After a
    // simulated main-isolate stall of `stallMs`, the worker isolate
    // MUST have delivered at least `floor((stallMs - jitter) / 1s)`
    // phase ticks — i.e. the indicator's wall-clock derivative is
    // never below 1 fps regardless of main-isolate availability.
    //
    // We allow 100 ms of jitter at the tail edge to absorb OS
    // scheduling slop on the worker isolate's `Timer.periodic`. For
    // small stalls (`stallMs <= 100`) the expected lower bound is 0
    // so the property is trivially satisfied — but the run still
    // exercises spawn/teardown, which is what we want to fuzz.
    //
    // numRuns is intentionally small: each run busy-waits up to 5 s
    // on the main thread, so 4 runs ≈ 20 s wall time per test.
    Glados(
      any.intInRange(0, 5001),
      ExploreConfig(numRuns: 4, initialSize: 1000, speed: 1000),
    ).test(
      'phase ticker advances >= floor((stallMs - 100ms) / 1s) under stall',
      (int stallMs) async {
        final probe = _LivenessProbe();
        await probe.start();
        // Wait for the first tick so the timer schedule is anchored
        // before we start the stall. Timer.periodic in the worker
        // first fires at +1 s; without anchoring, the very first
        // tick of any test run would be unpredictably aligned with
        // the start of the stall window.
        final anchor = Stopwatch()..start();
        while (probe.phases.isEmpty &&
            anchor.elapsed < const Duration(seconds: 3)) {
          await Future<void>.delayed(const Duration(milliseconds: 20));
        }
        anchor.stop();
        expect(probe.phases, isNotEmpty,
            reason: 'worker isolate must produce a tick within 3 s of spawn');

        final baseline = probe.phases.length;
        final sw = Stopwatch()..start();
        _stallMainIsolate(stallMs);
        final elapsedMs = sw.elapsedMilliseconds;
        sw.stop();
        // Drain any tick that fired during the busy-wait.
        await Future<void>.delayed(const Duration(milliseconds: 200));

        final delivered = probe.phases.length - baseline;
        final expected = ((elapsedMs - 100) ~/ 1000).clamp(0, 1 << 30);

        await probe.stop();

        expect(
          delivered,
          greaterThanOrEqualTo(expected),
          reason: 'stall=${stallMs}ms (actual ${elapsedMs}ms) → '
              'expected >= $expected ticks during stall, got $delivered. '
              'Worker isolate failed to keep pace with wall-clock while '
              'main isolate was blocked.',
        );
      },
    );

    // Phase increments must be monotone +1 mod 360. A blocked main
    // isolate must NOT cause spurious flips, replays, or skips: the
    // only observable values are exactly the side-channel worker's
    // wall-clock count.
    test('phase increments are monotone +1 mod 360 across a 2.5s stall',
        () async {
      final probe = _LivenessProbe();
      await probe.start();
      // Anchor on the first tick so we measure increments under
      // stall, not the first cold-start tick.
      final anchor = Stopwatch()..start();
      while (probe.phases.isEmpty &&
          anchor.elapsed < const Duration(seconds: 3)) {
        await Future<void>.delayed(const Duration(milliseconds: 20));
      }
      anchor.stop();

      _stallMainIsolate(2500);
      await Future<void>.delayed(const Duration(milliseconds: 250));
      await probe.stop();

      expect(probe.phases.length, greaterThanOrEqualTo(3),
          reason: '~3.5 s of wall time should yield at least 3 phase ticks');
      for (var i = 1; i < probe.phases.length; i++) {
        final prev = probe.phases[i - 1];
        final curr = probe.phases[i];
        expect(
          curr,
          equals((prev + 1) % 360),
          reason: 'phases must increment +1 mod 360 — got $prev → $curr',
        );
      }
    });

    // Restart of the worker isolate must be detected (the old port
    // is dead) and the new isolate must resync within a configurable
    // threshold. We pick 2 s = 1 tick interval + slack — slightly
    // more would be a 2-tick wait, which is more than the spec's
    // "≥1 fps" floor.
    test('phase ticker resyncs within 2s after worker isolate restart',
        () async {
      const resyncThreshold = Duration(seconds: 2);

      final first = _LivenessProbe();
      await first.start();
      final firstAnchor = Stopwatch()..start();
      while (first.phases.isEmpty &&
          firstAnchor.elapsed < const Duration(seconds: 3)) {
        await Future<void>.delayed(const Duration(milliseconds: 20));
      }
      firstAnchor.stop();
      expect(first.phases, isNotEmpty,
          reason: 'first worker must produce at least one tick before kill');
      await first.stop();

      final second = _LivenessProbe();
      final restartSw = Stopwatch()..start();
      await second.start();
      while (second.phases.isEmpty && restartSw.elapsed < resyncThreshold) {
        await Future<void>.delayed(const Duration(milliseconds: 50));
      }
      restartSw.stop();
      final timeToFirstTick = restartSw.elapsed;
      await second.stop();

      expect(second.phases, isNotEmpty,
          reason: 'restarted worker must produce a tick within '
              '$resyncThreshold');
      expect(
        timeToFirstTick,
        lessThanOrEqualTo(resyncThreshold),
        reason:
            'restart resync took $timeToFirstTick (threshold $resyncThreshold)',
      );
    });
  });
}
