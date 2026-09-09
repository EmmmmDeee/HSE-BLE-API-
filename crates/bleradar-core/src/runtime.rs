//! Rust-owned orchestration for the verified scan-loop cadence.
//!
//! Android remains responsible for invoking radio APIs and adapting callbacks.
//! This module owns the deterministic tick policy so scheduling decisions do not
//! have to be duplicated between the JVM service and native state.

/// Scan modes recovered from the Android service's scheduler contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    /// Scan BLE aggressively and run classic discovery every six ticks.
    Aggressive,
    /// The default mode, including invalid or empty persisted values.
    Balanced,
    /// Reduce radio work and run classic discovery every 24 ticks.
    LowPower,
}

impl ScanMode {
    /// Returns the BLE scanner mode ordinal used by the Android boundary.
    #[must_use]
    pub const fn ble_ordinal(self) -> u8 {
        match self {
            Self::Aggressive => 2,
            Self::Balanced => 1,
            Self::LowPower => 0,
        }
    }

    const fn classic_period(self) -> u64 {
        match self {
            Self::Aggressive => 6,
            Self::Balanced => 12,
            Self::LowPower => 24,
        }
    }
}

/// One side-effect request emitted by [`Runtime::advance_tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanCommand {
    /// Request a Wi-Fi scan.
    WifiScan,
    /// Request classic Bluetooth discovery.
    ClassicScan,
    /// Refresh threat candidates from the current store.
    RefreshThreats,
    /// Correlate the current observations.
    Correlate,
    /// Remove stale observations.
    Prune {
        /// Maximum observation age in milliseconds.
        max_age_ms: u64,
        /// Minimum sightings required to retain an observation.
        min_sightings: u16,
    },
    /// Stop and restart BLE scanning using the current mode.
    RearmBle {
        /// BLE scanner mode ordinal.
        mode: u8,
    },
}

/// Deterministic owner of the service scan-loop tick state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Runtime {
    mode: ScanMode,
    tick: u64,
}

impl Runtime {
    /// Creates a runtime beginning at oracle tick zero.
    #[must_use]
    pub const fn new(mode: ScanMode) -> Self {
        Self { mode, tick: 0 }
    }

    /// Returns the currently selected scan mode.
    #[must_use]
    pub const fn mode(self) -> ScanMode {
        self.mode
    }

    /// Changes the mode used by subsequent scheduling decisions.
    pub const fn set_mode(&mut self, mode: ScanMode) {
        self.mode = mode;
    }

    /// Returns the next oracle tick number that will be evaluated.
    #[must_use]
    pub const fn tick(self) -> u64 {
        self.tick
    }

    /// Advances one five-second service period and emits due actions.
    ///
    /// The current tick is evaluated before it is incremented, so tick zero
    /// starts Wi-Fi and the periodic actions retain the oracle's zero-based
    /// phase. Commands are returned in the verified order: Wi-Fi, classic,
    /// threat, correlation, prune, and BLE rearm.
    #[must_use]
    pub fn advance_tick(&mut self) -> Vec<ScanCommand> {
        let tick = self.tick;
        let mut commands = Vec::with_capacity(6);

        if tick.is_multiple_of(3) {
            commands.push(ScanCommand::WifiScan);
        }
        if tick.is_multiple_of(self.mode.classic_period()) {
            commands.push(ScanCommand::ClassicScan);
        }
        if tick.is_multiple_of(3) {
            commands.push(ScanCommand::RefreshThreats);
        }
        if tick.is_multiple_of(4) {
            commands.push(ScanCommand::Correlate);
        }
        if tick % 60 == 59 {
            commands.push(ScanCommand::Prune {
                max_age_ms: 30 * 60 * 1_000,
                min_sightings: 3,
            });
        }
        if tick % 240 == 239 {
            commands.push(ScanCommand::RearmBle {
                mode: self.mode.ble_ordinal(),
            });
        }

        self.tick = self.tick.wrapping_add(1);
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::{Runtime, ScanCommand, ScanMode};

    #[test]
    fn tick_zero_starts_wifi_and_preserves_order() {
        let mut runtime = Runtime::new(ScanMode::Aggressive);
        assert_eq!(
            runtime.advance_tick(),
            vec![
                ScanCommand::WifiScan,
                ScanCommand::ClassicScan,
                ScanCommand::RefreshThreats,
                ScanCommand::Correlate,
            ]
        );
        assert_eq!(runtime.tick(), 1);
    }

    #[test]
    fn mode_selects_classic_discovery_cadence_and_ble_ordinal() {
        for (mode, period, ordinal) in [
            (ScanMode::Aggressive, 6, 2),
            (ScanMode::Balanced, 12, 1),
            (ScanMode::LowPower, 24, 0),
        ] {
            let mut runtime = Runtime::new(mode);
            let mut classic_ticks = Vec::new();
            for tick in 0..period {
                let commands = runtime.advance_tick();
                if commands.contains(&ScanCommand::ClassicScan) {
                    classic_ticks.push(tick);
                }
            }
            assert_eq!(classic_ticks, vec![0]);

            runtime = Runtime::new(mode);
            for _ in 0..239 {
                let _ = runtime.advance_tick();
            }
            assert_eq!(
                runtime.advance_tick(),
                vec![
                    ScanCommand::Prune {
                        max_age_ms: 1_800_000,
                        min_sightings: 3,
                    },
                    ScanCommand::RearmBle { mode: ordinal },
                ]
            );
        }
    }

    #[test]
    fn periodic_actions_have_verified_phases() {
        let mut runtime = Runtime::new(ScanMode::Balanced);
        for _ in 0..59 {
            let _ = runtime.advance_tick();
        }
        assert_eq!(
            runtime.advance_tick(),
            vec![ScanCommand::Prune {
                max_age_ms: 1_800_000,
                min_sightings: 3,
            },]
        );
    }
}
