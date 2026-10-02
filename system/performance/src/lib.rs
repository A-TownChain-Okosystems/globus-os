//! GlobusOS Performance Center.
//!
//! Deterministic benchmark, tuning and optimization primitives. Hardware-specific
//! execution remains outside this crate and requires explicit capability boundaries.
#![no_std]

/// Workload classes supported by the initial performance contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Workload {
    /// General CPU computation.
    Cpu = 0,
    /// GPU compute or graphics workload.
    Gpu = 1,
    /// Main-memory bandwidth or latency workload.
    Memory = 2,
    /// Storage I/O workload.
    Storage = 3,
    /// Network throughput or latency workload.
    Network = 4,
    /// AI inference workload.
    Ai = 5,
}

/// A deterministic benchmark observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// Workload class measured.
    pub workload: Workload,
    /// Benchmark score; higher is better for the workload.
    pub score: u64,
    /// Observed operation latency in nanoseconds.
    pub latency_ns: u64,
    /// Utilization percentage in the inclusive range 0..=100.
    pub utilization_pct: u8,
    /// Observed power draw in milliwatts.
    pub power_mw: u32,
}

impl Sample {
    /// Creates a benchmark observation.
    pub const fn new(
        workload: Workload,
        score: u64,
        latency_ns: u64,
        utilization_pct: u8,
        power_mw: u32,
    ) -> Self {
        Self {
            workload,
            score,
            latency_ns,
            utilization_pct,
            power_mw,
        }
    }

    /// Calculates a deterministic score-per-power metric.
    pub const fn efficiency_score(&self) -> u64 {
        if self.power_mw == 0 {
            self.score
        } else {
            self.score.saturating_mul(1000) / self.power_mw as u64
        }
    }
}

/// Bottleneck classifications produced by the analysis stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Bottleneck {
    /// No actionable bottleneck detected.
    None = 0,
    /// Compute saturation.
    Compute = 1,
    /// Memory pressure or saturation.
    Memory = 2,
    /// Storage I/O saturation.
    Io = 3,
    /// Network saturation.
    Network = 4,
    /// Power-efficiency constraint.
    Power = 5,
    /// Latency constraint.
    Latency = 6,
}

/// Result of deterministic benchmark analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Analysis {
    /// Detected bottleneck.
    pub bottleneck: Bottleneck,
    /// Severity from 0 (none) to 2 (high).
    pub severity: u8,
}

/// Analyzes one benchmark observation without mutating system state.
pub const fn analyze(sample: &Sample) -> Analysis {
    let bottleneck = if sample.latency_ns > 10_000_000 {
        Bottleneck::Latency
    } else if sample.power_mw > 0
        && sample.score.saturating_mul(1000) / (sample.power_mw as u64) < 10
    {
        Bottleneck::Power
    } else if sample.utilization_pct >= 90 {
        match sample.workload {
            Workload::Memory => Bottleneck::Memory,
            Workload::Storage => Bottleneck::Io,
            Workload::Network => Bottleneck::Network,
            _ => Bottleneck::Compute,
        }
    } else {
        Bottleneck::None
    };

    let severity = match bottleneck {
        Bottleneck::None => 0,
        Bottleneck::Latency | Bottleneck::Power => 2,
        _ => 1,
    };

    Analysis {
        bottleneck,
        severity,
    }
}

/// Optimization actions that can be proposed by the planning stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Optimization {
    /// No optimization proposed.
    None,
    /// Reduce workload batch size.
    ReduceBatchSize,
    /// Adjust worker affinity through an authorized adapter.
    IncreaseWorkerAffinity,
    /// Offload memory pressure through an authorized adapter.
    EnableMemoryOffload,
    /// Reduce storage queue depth through an authorized adapter.
    ReduceIoQueueDepth,
    /// Select a power-constrained profile through an authorized adapter.
    EnablePowerProfile,
}

/// An optimization proposal. It is not an authorization to apply the change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Proposed action.
    pub optimization: Optimization,
    /// Expected performance delta in percentage points.
    pub expected_delta_pct: i8,
    /// Whether the proposed change must be benchmark-validated.
    pub requires_validation: bool,
}

/// Produces a deterministic optimization proposal from an analysis result.
pub const fn plan(analysis: Analysis, workload: Workload) -> Plan {
    let optimization = match (analysis.bottleneck, workload) {
        (Bottleneck::Compute, Workload::Ai) => Optimization::IncreaseWorkerAffinity,
        (Bottleneck::Memory, Workload::Ai) => Optimization::EnableMemoryOffload,
        (Bottleneck::Io, Workload::Storage) => Optimization::ReduceIoQueueDepth,
        (Bottleneck::Power, _) => Optimization::EnablePowerProfile,
        (Bottleneck::Latency, Workload::Ai) => Optimization::ReduceBatchSize,
        _ => Optimization::None,
    };

    let expected_delta_pct = match optimization {
        Optimization::None => 0,
        Optimization::IncreaseWorkerAffinity
        | Optimization::EnableMemoryOffload
        | Optimization::ReduceIoQueueDepth => 10,
        Optimization::ReduceBatchSize => 5,
        Optimization::EnablePowerProfile => -2,
    };

    Plan {
        optimization,
        expected_delta_pct,
        requires_validation: !matches!(optimization, Optimization::None),
    }
}

/// Comparison of a current benchmark with an explicit baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regression {
    /// Baseline score.
    pub baseline: u64,
    /// Current score.
    pub current: u64,
    /// Percentage delta from baseline.
    pub delta_pct: i16,
    /// Whether the negative delta exceeds the supplied threshold.
    pub regressed: bool,
}

/// Compares two scores against an explicit regression threshold.
pub const fn compare(baseline: u64, current: u64, threshold_pct: u8) -> Regression {
    if baseline == 0 {
        return Regression {
            baseline,
            current,
            delta_pct: 0,
            regressed: false,
        };
    }

    let delta_pct =
        ((current as i128 - baseline as i128) * 100 / baseline as i128) as i16;

    Regression {
        baseline,
        current,
        delta_pct,
        regressed: delta_pct < -(threshold_pct as i16),
    }
}

/// Stable system performance profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Profile {
    /// General-purpose policy.
    Balanced = 0,
    /// Latency-oriented policy.
    Gaming = 1,
    /// AI throughput-oriented policy.
    Ai = 2,
    /// Power-constrained policy.
    Battery = 3,
    /// Noise-constrained policy.
    Silent = 4,
    /// Development workload policy.
    Developer = 5,
}

/// Policy targets associated with a performance profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfilePolicy {
    /// Selected profile.
    pub profile: Profile,
    /// Optional power ceiling in milliwatts; zero means adapter-defined.
    pub max_power_mw: u32,
    /// Target latency in nanoseconds.
    pub target_latency_ns: u64,
}

/// Returns the stable policy contract for a profile.
pub const fn profile_policy(profile: Profile) -> ProfilePolicy {
    match profile {
        Profile::Balanced => ProfilePolicy {
            profile,
            max_power_mw: 0,
            target_latency_ns: 10_000_000,
        },
        Profile::Gaming => ProfilePolicy {
            profile,
            max_power_mw: 0,
            target_latency_ns: 5_000_000,
        },
        Profile::Ai => ProfilePolicy {
            profile,
            max_power_mw: 0,
            target_latency_ns: 20_000_000,
        },
        Profile::Battery => ProfilePolicy {
            profile,
            max_power_mw: 15_000,
            target_latency_ns: 30_000_000,
        },
        Profile::Silent => ProfilePolicy {
            profile,
            max_power_mw: 20_000,
            target_latency_ns: 15_000_000,
        },
        Profile::Developer => ProfilePolicy {
            profile,
            max_power_mw: 0,
            target_latency_ns: 10_000_000,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn efficiency_is_deterministic() {
        let sample = Sample::new(Workload::Ai, 2_000, 1_000, 80, 1_000);
        assert_eq!(sample.efficiency_score(), 2_000);
    }

    #[test]
    fn cpu_saturation_is_compute_bottleneck() {
        let sample = Sample::new(Workload::Cpu, 100, 1_000, 95, 1_000);
        assert_eq!(analyze(&sample).bottleneck, Bottleneck::Compute);
    }

    #[test]
    fn ai_memory_bottleneck_gets_offload_plan() {
        let plan = plan(
            Analysis {
                bottleneck: Bottleneck::Memory,
                severity: 1,
            },
            Workload::Ai,
        );
        assert_eq!(plan.optimization, Optimization::EnableMemoryOffload);
        assert!(plan.requires_validation);
    }

    #[test]
    fn regression_uses_explicit_threshold() {
        let result = compare(1_000, 900, 5);
        assert!(result.regressed);
        assert_eq!(result.delta_pct, -10);
    }

    #[test]
    fn profile_policies_are_stable() {
        assert_eq!(profile_policy(Profile::Battery).max_power_mw, 15_000);
        assert_eq!(
            profile_policy(Profile::Gaming).target_latency_ns,
            5_000_000
        );
    }
}
