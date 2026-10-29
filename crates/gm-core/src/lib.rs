//! Shared foundations: Source units, tick timing, ids.
//!
//! Everything in gmod-rust that touches world space uses Source units so that
//! numbers in the behaviour specs and in the parity harnesses need no conversion.

pub use glam;

pub mod units {
    //! Source engine units. One unit is 0.75 inch, 16 units are one foot.
    pub const INCHES_PER_UNIT: f32 = 0.75;
    pub const METERS_PER_UNIT: f32 = 0.0190_5;
    pub const UNITS_PER_METER: f32 = 1.0 / METERS_PER_UNIT;

    #[inline]
    pub fn to_meters(units: f32) -> f32 {
        units * METERS_PER_UNIT
    }

    #[inline]
    pub fn from_meters(meters: f32) -> f32 {
        meters * UNITS_PER_METER
    }
}

pub mod tick {
    //! Simulation tick. The default server tick rate is 66.67 Hz (15 ms).
    pub const DEFAULT_TICKRATE: f32 = 66.666_67;
    pub const DEFAULT_TICK_INTERVAL: f32 = 0.015;

    /// Tick interval for a given tick rate, the same way the original engine
    /// derives it: `1 / tickrate`, as f32.
    #[inline]
    pub fn interval(tickrate: f32) -> f32 {
        1.0 / tickrate
    }

    /// Convert seconds to a whole number of ticks, rounding to nearest.
    #[inline]
    pub fn seconds_to_ticks(seconds: f32, interval: f32) -> i32 {
        (seconds / interval + 0.5).floor() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_roundtrip() {
        let m = units::to_meters(64.0);
        assert!((m - 1.2192).abs() < 1e-4);
        assert!((units::from_meters(m) - 64.0).abs() < 1e-3);
    }

    #[test]
    fn tick_math() {
        let i = tick::interval(66.666_67);
        assert!((i - 0.015).abs() < 1e-6);
        assert_eq!(tick::seconds_to_ticks(1.0, i), 67);
        assert_eq!(tick::seconds_to_ticks(0.1, i), 7);
    }
}
