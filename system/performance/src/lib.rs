//! GlobusOS Performance Center.
//!
//! Deterministic benchmark, tuning and optimization primitives. Hardware-specific
//! execution remains outside this crate and requires explicit capability boundaries.
#![no_std]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Workload { Cpu=0, Gpu=1, Memory=2, Storage=3, Network=4, Ai=5 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample { pub workload: Workload, pub score: u64, pub latency_ns: u64, pub utilization_pct: u8, pub power_mw: u32 }
impl Sample {
    pub const fn new(workload: Workload, score: u64, latency_ns: u64, utilization_pct: u8, power_mw: u32) -> Self { Self { workload, score, latency_ns, utilization_pct, power_mw } }
    pub const fn efficiency_score(&self) -> u64 { if self.power_mw == 0 { self.score } else { self.score.saturating_mul(1000) / self.power_mw as u64 } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Bottleneck { None=0, Compute=1, Memory=2, Io=3, Network=4, Power=5, Latency=6 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Analysis { pub bottleneck: Bottleneck, pub severity: u8 }

pub const fn analyze(sample: &Sample) -> Analysis {
    let bottleneck = if sample.latency_ns > 10_000_000 { Bottleneck::Latency }
    else if sample.power_mw > 0 && sample.score.saturating_mul(1000) / sample.power_mw as u64 < 10 { Bottleneck::Power }
    else if sample.utilization_pct >= 90 { match sample.workload { Workload::Memory=>Bottleneck::Memory, Workload::Storage=>Bottleneck::Io, Workload::Network=>Bottleneck::Network, _=>Bottleneck::Compute } }
    else { Bottleneck::None };
    let severity = match bottleneck { Bottleneck::None=>0, Bottleneck::Latency|Bottleneck::Power=>2, _=>1 };
    Analysis { bottleneck, severity }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Optimization { None, ReduceBatchSize, IncreaseWorkerAffinity, EnableMemoryOffload, ReduceIoQueueDepth, EnablePowerProfile }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan { pub optimization: Optimization, pub expected_delta_pct: i8, pub requires_validation: bool }

pub const fn plan(analysis: Analysis, workload: Workload) -> Plan {
    let optimization = match (analysis.bottleneck, workload) {
        (Bottleneck::Compute, Workload::Ai) => Optimization::IncreaseWorkerAffinity,
        (Bottleneck::Memory, Workload::Ai) => Optimization::EnableMemoryOffload,
        (Bottleneck::Io, Workload::Storage) => Optimization::ReduceIoQueueDepth,
        (Bottleneck::Power, _) => Optimization::EnablePowerProfile,
        (Bottleneck::Latency, Workload::Ai) => Optimization::ReduceBatchSize,
        _ => Optimization::None,
    };
    let expected_delta_pct = match optimization { Optimization::None=>0, Optimization::IncreaseWorkerAffinity|Optimization::EnableMemoryOffload|Optimization::ReduceIoQueueDepth=>10, Optimization::ReduceBatchSize=>5, Optimization::EnablePowerProfile=>-2 };
    Plan { optimization, expected_delta_pct, requires_validation: !matches!(optimization, Optimization::None) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regression { pub baseline: u64, pub current: u64, pub delta_pct: i16, pub regressed: bool }
pub const fn compare(baseline: u64, current: u64, threshold_pct: u8) -> Regression {
    if baseline == 0 { return Regression { baseline, current, delta_pct:0, regressed:false }; }
    let delta_pct = ((current as i128 - baseline as i128) * 100 / baseline as i128) as i16;
    Regression { baseline, current, delta_pct, regressed: delta_pct < -(threshold_pct as i16) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Profile { Balanced=0, Gaming=1, Ai=2, Battery=3, Silent=4, Developer=5 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfilePolicy { pub profile: Profile, pub max_power_mw: u32, pub target_latency_ns: u64 }
pub const fn profile_policy(profile: Profile) -> ProfilePolicy { match profile {
    Profile::Balanced=>ProfilePolicy{profile,max_power_mw:0,target_latency_ns:10_000_000}, Profile::Gaming=>ProfilePolicy{profile,max_power_mw:0,target_latency_ns:5_000_000}, Profile::Ai=>ProfilePolicy{profile,max_power_mw:0,target_latency_ns:20_000_000}, Profile::Battery=>ProfilePolicy{profile,max_power_mw:15_000,target_latency_ns:30_000_000}, Profile::Silent=>ProfilePolicy{profile,max_power_mw:20_000,target_latency_ns:15_000_000}, Profile::Developer=>ProfilePolicy{profile,max_power_mw:0,target_latency_ns:10_000_000},
} }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn efficiency_is_deterministic() { let s=Sample::new(Workload::Ai,2_000,1_000,80,1_000); assert_eq!(s.efficiency_score(),2_000); }
    #[test] fn cpu_saturation_is_compute_bottleneck() { let s=Sample::new(Workload::Cpu,100,1_000,95,1_000); assert_eq!(analyze(&s).bottleneck,Bottleneck::Compute); }
    #[test] fn ai_memory_bottleneck_gets_offload_plan() { let p=plan(Analysis{bottleneck:Bottleneck::Memory,severity:1},Workload::Ai); assert_eq!(p.optimization,Optimization::EnableMemoryOffload); assert!(p.requires_validation); }
    #[test] fn regression_uses_explicit_threshold() { let r=compare(1_000,900,5); assert!(r.regressed); assert_eq!(r.delta_pct,-10); }
    #[test] fn profile_policies_are_stable() { assert_eq!(profile_policy(Profile::Battery).max_power_mw,15_000); assert_eq!(profile_policy(Profile::Gaming).target_latency_ns,5_000_000); }
}
