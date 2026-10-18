//! Dependency-aware service supervisor state machine.
//!
//! This module models lifecycle policy only. It does not spawn processes, grant
//! kernel capabilities, or prove that a service is healthy; those operations
//! must be supplied by the platform's process/IPC adapter.

use std::collections::{BTreeMap, BTreeSet};

use crate::{ServiceSpec, ServiceState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorError {
    EmptyName,
    DuplicateService(String),
    MissingDependency { service: String, dependency: String },
    DependencyCycle(String),
    UnknownService(String),
    InvalidTransition { service: String, state: ServiceState },
    DependenciesNotReady { service: String, dependencies: Vec<String> },
    ActiveDependents { service: String, dependents: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSnapshot {
    pub name: String,
    pub state: ServiceState,
    pub critical: bool,
    pub restart_count: u32,
}

/// Deterministic lifecycle coordinator. Callers perform the actual process
/// start/stop and report outcomes through mark_ready / mark_failed.
pub struct ServiceSupervisor {
    specs: BTreeMap<String, ServiceSpec>,
    states: BTreeMap<String, ServiceState>,
    restart_counts: BTreeMap<String, u32>,
}

impl ServiceSupervisor {
    pub fn new(specs: impl IntoIterator<Item = ServiceSpec>) -> Result<Self, SupervisorError> {
        let mut by_name = BTreeMap::new();
        for spec in specs {
            if spec.name.trim().is_empty() {
                return Err(SupervisorError::EmptyName);
            }
            let name = spec.name.clone();
            if by_name.insert(name.clone(), spec).is_some() {
                return Err(SupervisorError::DuplicateService(name));
            }
        }

        for (name, spec) in &by_name {
            for dependency in &spec.dependencies {
                if !by_name.contains_key(dependency) {
                    return Err(SupervisorError::MissingDependency {
                        service: name.clone(),
                        dependency: dependency.clone(),
                    });
                }
                if dependency == name {
                    return Err(SupervisorError::DependencyCycle(name.clone()));
                }
            }
        }

        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for name in by_name.keys() {
            Self::visit(name, &by_name, &mut visiting, &mut visited)?;
        }

        let states = by_name
            .keys()
            .map(|name| (name.clone(), ServiceState::Defined))
            .collect();
        let restart_counts = by_name.keys().map(|name| (name.clone(), 0)).collect();

        Ok(Self {
            specs: by_name,
            states,
            restart_counts,
        })
    }

    fn visit(
        name: &str,
        specs: &BTreeMap<String, ServiceSpec>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<(), SupervisorError> {
        if visited.contains(name) {
            return Ok(());
        }
        if !visiting.insert(name.to_owned()) {
            return Err(SupervisorError::DependencyCycle(name.to_owned()));
        }
        if let Some(spec) = specs.get(name) {
            for dependency in &spec.dependencies {
                Self::visit(dependency, specs, visiting, visited)?;
            }
        }
        visiting.remove(name);
        visited.insert(name.to_owned());
        Ok(())
    }

    /// Returns services whose dependencies are ready and which can be started.
    pub fn startable(&self) -> Vec<String> {
        self.specs
            .iter()
            .filter(|(name, spec)| {
                self.states.get(*name) == Some(&ServiceState::Defined)
                    && spec.dependencies.iter().all(|dependency| {
                        self.states.get(dependency) == Some(&ServiceState::Ready)
                    })
            })
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn begin_start(&mut self, name: &str) -> Result<(), SupervisorError> {
        let spec = self
            .specs
            .get(name)
            .ok_or_else(|| SupervisorError::UnknownService(name.to_owned()))?;
        let state = *self.states.get(name).expect("state exists for every spec");
        if state != ServiceState::Defined && state != ServiceState::Stopped {
            return Err(SupervisorError::InvalidTransition {
                service: name.to_owned(),
                state,
            });
        }
        let dependencies: Vec<String> = spec
            .dependencies
            .iter()
            .filter(|dependency| {
                self.states.get(*dependency) != Some(&ServiceState::Ready)
            })
            .cloned()
            .collect();
        if !dependencies.is_empty() {
            return Err(SupervisorError::DependenciesNotReady {
                service: name.to_owned(),
                dependencies,
            });
        }
        self.states.insert(name.to_owned(), ServiceState::Starting);
        Ok(())
    }

    pub fn mark_ready(&mut self, name: &str) -> Result<(), SupervisorError> {
        self.transition(name, ServiceState::Starting, ServiceState::Ready)
    }

    pub fn mark_failed(&mut self, name: &str) -> Result<(), SupervisorError> {
        let state = *self
            .states
            .get(name)
            .ok_or_else(|| SupervisorError::UnknownService(name.to_owned()))?;
        if state != ServiceState::Starting && state != ServiceState::Ready {
            return Err(SupervisorError::InvalidTransition {
                service: name.to_owned(),
                state,
            });
        }
        self.states.insert(name.to_owned(), ServiceState::Failed);
        Ok(())
    }

    pub fn stop(&mut self, name: &str) -> Result<(), SupervisorError> {
        let state = *self
            .states
            .get(name)
            .ok_or_else(|| SupervisorError::UnknownService(name.to_owned()))?;
        if state != ServiceState::Ready && state != ServiceState::Failed {
            return Err(SupervisorError::InvalidTransition {
                service: name.to_owned(),
                state,
            });
        }
        let dependents: Vec<String> = self
            .specs
            .iter()
            .filter(|(candidate, spec)| {
                candidate.as_str() != name
                    && spec.dependencies.iter().any(|dependency| dependency == name)
                    && self.states.get(*candidate) == Some(&ServiceState::Ready)
            })
            .map(|(candidate, _)| candidate.clone())
            .collect();
        if !dependents.is_empty() {
            return Err(SupervisorError::ActiveDependents {
                service: name.to_owned(),
                dependents,
            });
        }
        self.states.insert(name.to_owned(), ServiceState::Stopped);
        Ok(())
    }

    /// Records a caller-confirmed restart attempt after a failure.
    /// This does not itself launch a process or implement backoff.
    pub fn schedule_restart(&mut self, name: &str) -> Result<u32, SupervisorError> {
        let state = *self
            .states
            .get(name)
            .ok_or_else(|| SupervisorError::UnknownService(name.to_owned()))?;
        if state != ServiceState::Failed && state != ServiceState::Stopped {
            return Err(SupervisorError::InvalidTransition {
                service: name.to_owned(),
                state,
            });
        }
        let count = self
            .restart_counts
            .get_mut(name)
            .expect("restart count exists for every spec");
        *count = count.saturating_add(1);
        self.states.insert(name.to_owned(), ServiceState::Defined);
        Ok(*count)
    }

    pub fn snapshot(&self, name: &str) -> Result<ServiceSnapshot, SupervisorError> {
        let spec = self
            .specs
            .get(name)
            .ok_or_else(|| SupervisorError::UnknownService(name.to_owned()))?;
        Ok(ServiceSnapshot {
            name: name.to_owned(),
            state: *self.states.get(name).expect("state exists for every spec"),
            critical: spec.critical,
            restart_count: *self
                .restart_counts
                .get(name)
                .expect("restart count exists for every spec"),
        })
    }

    fn transition(
        &mut self,
        name: &str,
        expected: ServiceState,
        next: ServiceState,
    ) -> Result<(), SupervisorError> {
        let state = *self
            .states
            .get(name)
            .ok_or_else(|| SupervisorError::UnknownService(name.to_owned()))?;
        if state != expected {
            return Err(SupervisorError::InvalidTransition {
                service: name.to_owned(),
                state,
            });
        }
        self.states.insert(name.to_owned(), next);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, dependencies: &[&str], critical: bool) -> ServiceSpec {
        ServiceSpec {
            name: name.to_owned(),
            dependencies: dependencies.iter().map(|value| (*value).to_owned()).collect(),
            critical,
        }
    }

    #[test]
    fn services_start_only_after_dependencies_are_ready() {
        let mut supervisor = ServiceSupervisor::new([
            spec("security", &[], true),
            spec("network", &["security"], false),
        ])
        .unwrap();

        assert_eq!(supervisor.startable(), vec!["security"]);
        assert!(matches!(
            supervisor.begin_start("network"),
            Err(SupervisorError::DependenciesNotReady { .. })
        ));
        supervisor.begin_start("security").unwrap();
        supervisor.mark_ready("security").unwrap();
        assert_eq!(supervisor.startable(), vec!["network"]);
    }

    #[test]
    fn missing_dependencies_and_cycles_are_rejected() {
        assert!(matches!(
            ServiceSupervisor::new([spec("network", &["devices"], false)]),
            Err(SupervisorError::MissingDependency { .. })
        ));
        assert!(matches!(
            ServiceSupervisor::new([
                spec("a", &["b"], true),
                spec("b", &["a"], true),
            ]),
            Err(SupervisorError::DependencyCycle(_))
        ));
    }

    #[test]
    fn active_dependents_prevent_unsafe_stop() {
        let mut supervisor = ServiceSupervisor::new([
            spec("security", &[], true),
            spec("network", &["security"], false),
        ])
        .unwrap();
        supervisor.begin_start("security").unwrap();
        supervisor.mark_ready("security").unwrap();
        supervisor.begin_start("network").unwrap();
        supervisor.mark_ready("network").unwrap();

        assert!(matches!(
            supervisor.stop("security"),
            Err(SupervisorError::ActiveDependents { .. })
        ));
    }

    #[test]
    fn failure_and_restart_attempts_are_explicit() {
        let mut supervisor = ServiceSupervisor::new([spec("audio", &[], false)]).unwrap();
        supervisor.begin_start("audio").unwrap();
        supervisor.mark_failed("audio").unwrap();
        assert_eq!(supervisor.schedule_restart("audio"), Ok(1));
        assert_eq!(supervisor.snapshot("audio").unwrap().state, ServiceState::Defined);
    }

    #[test]
    fn duplicate_and_empty_names_are_rejected() {
        assert_eq!(
            ServiceSupervisor::new([spec("", &[], true)]).unwrap_err(),
            SupervisorError::EmptyName
        );
        assert!(matches!(
            ServiceSupervisor::new([spec("security", &[], true), spec("security", &[], false)]),
            Err(SupervisorError::DuplicateService(_))
        ));
    }
}
