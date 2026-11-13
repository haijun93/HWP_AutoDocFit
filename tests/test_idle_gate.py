import threading
import unittest

from docfit_app.idle_gate import (IdleConfig, IdleGate, IdleSignals, background_priority, clamp_idle_seconds,
                                  cpu_percent_from_times, decide)

QUIET = IdleSignals(input_idle_seconds=600, cpu_percent=5, disk_bytes_per_second=0, on_battery=False, fullscreen=False)


class FakeProbe:
    def __init__(self, signals):
        self.signals = signals
        self.calls = 0

    def sample(self):
        self.calls += 1
        return self.signals


class Clock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


class DecideTests(unittest.TestCase):
    def test_quiet_machine_is_idle(self):
        self.assertEqual(decide(None, QUIET, IdleConfig()).state, "idle")

    def test_disabled_wins_over_everything(self):
        result = decide("idle", QUIET, IdleConfig(enabled=False))
        self.assertEqual((result.state, result.reason), ("paused", "disabled"))

    def test_recent_input_is_active(self):
        signals = IdleSignals(input_idle_seconds=3, cpu_percent=0)
        self.assertEqual(decide("idle", signals, IdleConfig()).state, "active")

    def test_input_threshold_uses_setting(self):
        signals = IdleSignals(input_idle_seconds=40)
        self.assertEqual(decide(None, signals, IdleConfig(idle_seconds=30)).state, "idle")
        self.assertEqual(decide(None, signals, IdleConfig(idle_seconds=60)).state, "active")

    def test_unknown_input_holds(self):
        result = decide(None, IdleSignals(), IdleConfig())
        self.assertEqual((result.state, result.reason), ("held", "input_unknown"))

    def test_battery_and_fullscreen_hold(self):
        self.assertEqual(decide(None, IdleSignals(input_idle_seconds=600, on_battery=True), IdleConfig()).reason, "battery")
        self.assertEqual(decide(None, IdleSignals(input_idle_seconds=600, fullscreen=True), IdleConfig()).reason, "fullscreen")

    def test_load_hysteresis(self):
        busy = IdleSignals(input_idle_seconds=600, cpu_percent=40)
        # 들어갈 때는 25%를 넘으면 보류, 이미 쉬는 중이면 60%까지 버틴다.
        self.assertEqual(decide("held", busy, IdleConfig()).reason, "cpu")
        self.assertEqual(decide("idle", busy, IdleConfig()).state, "idle")
        self.assertEqual(decide("idle", IdleSignals(input_idle_seconds=600, cpu_percent=70), IdleConfig()).reason, "cpu")
        disk = IdleSignals(input_idle_seconds=600, disk_bytes_per_second=16 * 1024 * 1024)
        self.assertEqual(decide("active", disk, IdleConfig()).reason, "disk")
        self.assertEqual(decide("idle", disk, IdleConfig()).state, "idle")

    def test_unknown_load_is_ignored(self):
        self.assertEqual(decide(None, IdleSignals(input_idle_seconds=600), IdleConfig()).state, "idle")

    def test_cpu_percent_excludes_own_process(self):
        # 총 1000 중 유휴 600, 이 프로세스 300 → 다른 프로그램 100 = 10%
        self.assertAlmostEqual(cpu_percent_from_times((0, 0, 0), (600, 1000, 300)), 10.0)
        self.assertIsNone(cpu_percent_from_times((5, 5, 5), (5, 5, 5)))

    def test_clamp_idle_seconds(self):
        self.assertEqual(clamp_idle_seconds(120), 120)
        for bad in (True, 1.5, "60", 5, 99999):
            with self.assertRaises(ValueError):
                clamp_idle_seconds(bad)


class IdleGateTests(unittest.TestCase):
    def test_poll_caches_within_interval_and_counts_transitions(self):
        probe, clock = FakeProbe(QUIET), Clock()
        gate = IdleGate(probe, clock=clock, sample_interval=1.0)
        self.assertTrue(gate.is_idle())
        gate.poll()
        self.assertEqual(probe.calls, 1)
        probe.signals = IdleSignals(input_idle_seconds=0.5)
        self.assertTrue(gate.is_idle())  # 아직 같은 측정 구간
        clock.now = 1.5
        self.assertEqual(gate.poll().state, "active")
        snapshot = gate.snapshot()
        self.assertEqual((snapshot["state"], snapshot["reason_text"], snapshot["transitions"]), ("active", "사용 중", 2))

    def test_configure_rejudges_immediately(self):
        gate = IdleGate(FakeProbe(QUIET), clock=Clock())
        self.assertEqual(gate.poll().state, "idle")
        self.assertEqual(gate.configure(enabled=False).state, "paused")
        self.assertEqual(gate.configure(enabled=True, idle_seconds=900).state, "active")
        self.assertEqual(gate.snapshot()["idle_seconds"], 900)
        with self.assertRaises(ValueError):
            gate.configure(idle_seconds=1)

    def test_wait_until_idle_stop_and_bypass(self):
        gate = IdleGate(FakeProbe(IdleSignals(input_idle_seconds=0)), clock=Clock(), sample_interval=0)
        stop, bypass = threading.Event(), threading.Event()
        stop.set()
        self.assertFalse(gate.wait_until_idle(stop, poll_seconds=0.01))
        bypass.set()
        self.assertTrue(gate.wait_until_idle(threading.Event(), bypass=bypass, poll_seconds=0.01))

    def test_wait_until_idle_returns_when_user_leaves(self):
        probe = FakeProbe(IdleSignals(input_idle_seconds=0))
        gate = IdleGate(probe, clock=Clock(), sample_interval=0)
        timer = threading.Timer(0.05, lambda: setattr(probe, "signals", QUIET))
        timer.start()
        try:
            self.assertTrue(gate.wait_until_idle(threading.Event(), poll_seconds=0.01))
        finally:
            timer.cancel()

    def test_background_priority_is_safe_everywhere(self):
        with background_priority() as started:
            self.assertIsInstance(started, bool)


if __name__ == "__main__":
    unittest.main()
