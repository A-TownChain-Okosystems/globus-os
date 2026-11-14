//! Monotonic timer and scheduler-tick contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Frequenz des monotonen Zeitgebers.
pub struct TimerFrequency {
    /// Frequenz in Hertz.
    pub hz: u64,
}

impl TimerFrequency {
    /// True, wenn die Frequenz groesser 0 ist.
    pub fn valid(self) -> bool {
        self.hz > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein abgelaufener Tick.
pub struct TimerTick {
    /// Fortlaufende Nummer des Ticks.
    pub sequence: u64,
    /// Seit dem Start verstrichene Nanosekunden.
    pub elapsed_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler des Timers.
pub enum TimerError {
    /// Die Frequenz ist 0.
    InvalidFrequency,
    /// Die Zeitrechnung laeuft ueber.
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Monoton laufender Timer.
pub struct MonotonicTimer {
    frequency: TimerFrequency,
    ticks: u64,
}

impl MonotonicTimer {
    /// Legt einen Timer mit gueltiger Frequenz an.
    pub fn new(frequency: TimerFrequency) -> Result<Self, TimerError> {
        if !frequency.valid() {
            return Err(TimerError::InvalidFrequency);
        }
        Ok(Self {
            frequency,
            ticks: 0,
        })
    }

    /// Frequenz des Timers.
    pub fn frequency(&self) -> TimerFrequency {
        self.frequency
    }

    /// Zaehlt einen Tick und liefert seine Nummer und Zeit.
    pub fn tick(&mut self) -> Result<TimerTick, TimerError> {
        self.ticks = self.ticks.checked_add(1).ok_or(TimerError::Overflow)?;
        let elapsed_ns = self
            .ticks
            .checked_mul(1_000_000_000 / self.frequency.hz)
            .ok_or(TimerError::Overflow)?;
        Ok(TimerTick {
            sequence: self.ticks,
            elapsed_ns,
        })
    }

    /// Bisher gezaehlte Ticks.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timer_is_monotonic() {
        let mut timer = MonotonicTimer::new(TimerFrequency { hz: 1000 }).unwrap();
        assert_eq!(timer.tick().unwrap().sequence, 1);
        assert_eq!(timer.tick().unwrap().sequence, 2);
        assert!(timer.tick().unwrap().elapsed_ns > 0);
    }
    #[test]
    fn rejects_zero_frequency() {
        assert_eq!(
            MonotonicTimer::new(TimerFrequency { hz: 0 }),
            Err(TimerError::InvalidFrequency)
        );
    }
}
