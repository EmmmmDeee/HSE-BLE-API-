//! Integration tests for the scan-loop [`Runtime`] through the crate's public
//! API. The module's own unit tests pin tick zero and the 60/240-tick phases;
//! these cover what they leave open: `plan_tick` being a pure preview of
//! `advance_tick`, `set_mode` taking effect on the next tick without
//! resetting the phase, and the exact per-command counts over a full
//! 240-tick (twenty-minute) cycle in every mode.

use bleradar_core::{Runtime, ScanCommand, ScanMode};

const ALL_MODES: [ScanMode; 3] = [ScanMode::Aggressive, ScanMode::Balanced, ScanMode::LowPower];

fn count(commands: &[Vec<ScanCommand>], wanted: fn(&ScanCommand) -> bool) -> usize {
    commands
        .iter()
        .map(|tick| tick.iter().filter(|c| wanted(c)).count())
        .sum()
}

#[test]
fn ble_ordinals_are_the_android_scan_mode_constants() {
    // ScanSettings.SCAN_MODE_LOW_POWER = 0, BALANCED = 1, LOW_LATENCY = 2.
    assert_eq!(ScanMode::LowPower.ble_ordinal(), 0);
    assert_eq!(ScanMode::Balanced.ble_ordinal(), 1);
    assert_eq!(ScanMode::Aggressive.ble_ordinal(), 2);
}

#[test]
fn a_new_runtime_starts_at_tick_zero_in_the_requested_mode() {
    for mode in ALL_MODES {
        let runtime = Runtime::new(mode);
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.mode(), mode);
    }
}

#[test]
fn plan_tick_previews_advance_tick_without_moving_the_clock() {
    for mode in ALL_MODES {
        let mut runtime = Runtime::new(mode);
        for expected_tick in 0..500 {
            assert_eq!(runtime.tick(), expected_tick);
            let first = runtime.plan_tick();
            // Planning is idempotent and does not advance.
            assert_eq!(runtime.plan_tick(), first);
            assert_eq!(runtime.tick(), expected_tick);
            // Advancing emits exactly what was planned, then moves on by one.
            assert_eq!(
                runtime.advance_tick(),
                first,
                "{mode:?} tick {expected_tick}"
            );
            assert_eq!(runtime.tick(), expected_tick + 1);
        }
    }
}

#[test]
fn a_full_cycle_emits_each_command_the_documented_number_of_times() {
    for (mode, classic_scans) in [
        (ScanMode::Aggressive, 240 / 6),
        (ScanMode::Balanced, 240 / 12),
        (ScanMode::LowPower, 240 / 24),
    ] {
        let mut runtime = Runtime::new(mode);
        let cycle: Vec<Vec<ScanCommand>> = (0..240).map(|_| runtime.advance_tick()).collect();
        assert_eq!(count(&cycle, |c| *c == ScanCommand::WifiScan), 80);
        assert_eq!(count(&cycle, |c| *c == ScanCommand::RefreshThreats), 80);
        assert_eq!(count(&cycle, |c| *c == ScanCommand::Correlate), 60);
        assert_eq!(
            count(&cycle, |c| *c == ScanCommand::ClassicScan),
            classic_scans,
            "{mode:?}"
        );
        assert_eq!(count(&cycle, |c| matches!(c, ScanCommand::Prune { .. })), 4);
        assert_eq!(
            count(&cycle, |c| matches!(c, ScanCommand::RearmBle { .. })),
            1
        );
        // The schedule is periodic: the next cycle repeats this one exactly.
        let next: Vec<Vec<ScanCommand>> = (0..240).map(|_| runtime.advance_tick()).collect();
        assert_eq!(next, cycle, "{mode:?}");
    }
}

#[test]
fn every_planned_tick_keeps_the_verified_command_order() {
    fn rank(command: &ScanCommand) -> u8 {
        match command {
            ScanCommand::WifiScan => 0,
            ScanCommand::ClassicScan => 1,
            ScanCommand::RefreshThreats => 2,
            ScanCommand::Correlate => 3,
            ScanCommand::Prune { .. } => 4,
            ScanCommand::RearmBle { .. } => 5,
        }
    }
    for mode in ALL_MODES {
        let mut runtime = Runtime::new(mode);
        for tick in 0..480 {
            let commands = runtime.advance_tick();
            let ranks: Vec<u8> = commands.iter().map(rank).collect();
            assert!(
                ranks.windows(2).all(|pair| pair[0] < pair[1]),
                "{mode:?} tick {tick}: {commands:?}"
            );
        }
    }
}

#[test]
fn set_mode_applies_to_the_next_tick_without_resetting_the_phase() {
    let mut runtime = Runtime::new(ScanMode::LowPower);
    for _ in 0..6 {
        let _ = runtime.advance_tick();
    }
    // Tick 6: LowPower (every 24) would not run classic discovery here.
    assert!(!runtime.plan_tick().contains(&ScanCommand::ClassicScan));

    runtime.set_mode(ScanMode::Aggressive);
    assert_eq!(runtime.mode(), ScanMode::Aggressive);
    assert_eq!(runtime.tick(), 6, "changing mode must not reset the clock");
    // Aggressive (every 6) runs it at the very next evaluated tick.
    assert!(runtime.advance_tick().contains(&ScanCommand::ClassicScan));

    // The BLE re-arm at tick 239 carries the mode current at that moment.
    while runtime.tick() < 239 {
        let _ = runtime.advance_tick();
    }
    runtime.set_mode(ScanMode::Balanced);
    let commands = runtime.advance_tick();
    assert!(
        commands.contains(&ScanCommand::RearmBle {
            mode: ScanMode::Balanced.ble_ordinal()
        }),
        "{commands:?}"
    );
}

#[test]
fn prune_always_carries_the_thirty_minute_three_sighting_policy() {
    let mut runtime = Runtime::new(ScanMode::Balanced);
    let mut prunes = Vec::new();
    for tick in 0..1_000u64 {
        for command in runtime.advance_tick() {
            if let ScanCommand::Prune {
                max_age_ms,
                min_sightings,
            } = command
            {
                assert_eq!(max_age_ms, 30 * 60 * 1_000);
                assert_eq!(min_sightings, 3);
                prunes.push(tick);
            }
        }
    }
    let expected: Vec<u64> = (0..1_000).filter(|t| t % 60 == 59).collect();
    assert_eq!(prunes, expected);
}
