//! Boot-time orchestration between the dependency supervisor and a platform adapter.
//!
//! This crate defines init orchestration. The platform adapter must bind the
//! process IDs and lifecycle operations to the kernel's supported process runtime.

use globus_services::{ServiceSpec, ServiceState, ServiceSupervisor, SupervisorError};
use libshivacore::{AbiHandshake, CapabilityHandle, ProcessId};

#[derive(Debug, PartialEq, Eq)]
pub enum ServiceBootError {
    AbiVersionMismatch { major: u16, minor: u16 },
    Supervisor(SupervisorError),
    Adapter(String),
    HealthCheckFailed(String),
    MissingSpecification(String),
}

/// Platform-specific operations required by `globus-init`.
///
/// `launch` must create a non-runnable/staged process. The adapter then grants
/// only policy-approved capabilities to that PID and calls `activate`; it must
/// not expose the process to the scheduler before capability handoff completes.
pub trait ServiceProcessAdapter {
    fn abi_handshake(&mut self) -> Result<AbiHandshake, String>;
    fn launch(&mut self, service: &ServiceSpec) -> Result<ProcessId, String>;
    fn initial_capabilities(
        &mut self,
        service: &ServiceSpec,
        process: ProcessId,
    ) -> Result<Vec<CapabilityHandle>, String>;
    fn activate(
        &mut self,
        service: &ServiceSpec,
        process: ProcessId,
        capabilities: &[CapabilityHandle],
    ) -> Result<(), String>;
    fn is_healthy(&mut self, service: &ServiceSpec, process: ProcessId) -> Result<bool, String>;
    fn terminate(&mut self, service: &ServiceSpec, process: ProcessId) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedService {
    pub name: String,
    pub process_id: ProcessId,
}

/// Starts every dependency-ready service in deterministic waves.
///
/// A service is marked `Ready` only after the adapter's health check succeeds.
/// This function does not implement process creation itself; it drives the
/// platform adapter and refuses to proceed on an incompatible major ABI.
pub fn start_ready_services(
    supervisor: &mut ServiceSupervisor,
    specs: &[ServiceSpec],
    adapter: &mut impl ServiceProcessAdapter,
) -> Result<Vec<StartedService>, ServiceBootError> {
    let handshake = adapter.abi_handshake().map_err(ServiceBootError::Adapter)?;
    if !handshake.compatible() {
        return Err(ServiceBootError::AbiVersionMismatch {
            major: handshake.major,
            minor: handshake.minor,
        });
    }

    let mut started = Vec::new();
    loop {
        let candidates = supervisor.startable();
        if candidates.is_empty() {
            break;
        }
        let mut progressed = false;
        for name in candidates {
            let spec = specs
                .iter()
                .find(|spec| spec.name == name)
                .ok_or_else(|| ServiceBootError::MissingSpecification(name.clone()))?;

            supervisor
                .begin_start(&name)
                .map_err(ServiceBootError::Supervisor)?;
            let process_id = match adapter.launch(spec) {
                Ok(process_id) => process_id,
                Err(error) => {
                    supervisor.mark_failed(&name).map_err(ServiceBootError::Supervisor)?;
                    return Err(ServiceBootError::Adapter(error));
                }
            };
            let capabilities = match adapter.initial_capabilities(&name, process_id) {
                Ok(capabilities) => capabilities,
                Err(error) => {
                    let _ = adapter.terminate(spec, process_id);
                    supervisor.mark_failed(&name).map_err(ServiceBootError::Supervisor)?;
                    return Err(ServiceBootError::Adapter(error));
                }
            };
            if let Err(error) = adapter.activate(spec, process_id, &capabilities) {
                let _ = adapter.terminate(spec, process_id);
                supervisor.mark_failed(&name).map_err(ServiceBootError::Supervisor)?;
                return Err(ServiceBootError::Adapter(error));
            }
            match adapter.is_healthy(spec, process_id) {
                Ok(true) => {
                    supervisor.mark_ready(&name).map_err(ServiceBootError::Supervisor)?;
                    started.push(StartedService { name, process_id });
                    progressed = true;
                }
                Ok(false) => {
                    let termination = adapter.terminate(spec, process_id);
                    supervisor.mark_failed(&name).map_err(ServiceBootError::Supervisor)?;
                    if let Err(error) = termination {
                        return Err(ServiceBootError::Adapter(error));
                    }
                    return Err(ServiceBootError::HealthCheckFailed(name));
                }
                Err(error) => {
                    let _ = adapter.terminate(spec, process_id);
                    supervisor.mark_failed(&name).map_err(ServiceBootError::Supervisor)?;
                    return Err(ServiceBootError::Adapter(error));
                }
            }
        }
        if !progressed {
            break;
        }
    }

    Ok(started)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAdapter {
        handshake: AbiHandshake,
        launched: Vec<String>,
        unhealthy: Option<String>,
    }

    impl ServiceProcessAdapter for MockAdapter {
        fn abi_handshake(&mut self) -> Result<AbiHandshake, String> {
            Ok(self.handshake)
        }

        fn launch(&mut self, service: &ServiceSpec) -> Result<ProcessId, String> {
            self.launched.push(service.name.clone());
            Ok(ProcessId(self.launched.len() as u32 + 1000))
        }

        fn initial_capabilities(
            &mut self,
            _service: &str,
            _process: ProcessId,
        ) -> Result<Vec<CapabilityHandle>, String> {
            Ok(Vec::new())
        }

        fn activate(
            &mut self,
            _service: &ServiceSpec,
            _process: ProcessId,
            _capabilities: &[CapabilityHandle],
        ) -> Result<(), String> {
            Ok(())
        }

        fn is_healthy(&mut self, service: &ServiceSpec, _process: ProcessId) -> Result<bool, String> {
            Ok(self.unhealthy.as_deref() != Some(service.name.as_str()))
        }

        fn terminate(&mut self, _service: &ServiceSpec, _process: ProcessId) -> Result<(), String> {
            Ok(())
        }
    }

    fn spec(name: &str, dependencies: &[&str]) -> ServiceSpec {
        ServiceSpec {
            name: name.to_owned(),
            dependencies: dependencies.iter().map(|item| (*item).to_owned()).collect(),
            critical: false,
        }
    }

    #[test]
    fn launches_services_in_dependency_order_and_checks_health() {
        let specs = vec![spec("network", &["security"]), spec("security", &[])];
        let mut supervisor = ServiceSupervisor::new(specs.clone()).unwrap();
        let mut adapter = MockAdapter {
            handshake: AbiHandshake::CURRENT,
            launched: Vec::new(),
            unhealthy: None,
        };

        let started = start_ready_services(&mut supervisor, &specs, &mut adapter).unwrap();
        assert_eq!(
            started.iter().map(|service| service.name.as_str()).collect::<Vec<_>>(),
            vec!["security", "network"]
        );
        assert_eq!(supervisor.snapshot("network").unwrap().state, ServiceState::Ready);
    }

    #[test]
    fn incompatible_abi_is_rejected_before_launch() {
        let specs = vec![spec("security", &[])];
        let mut supervisor = ServiceSupervisor::new(specs.clone()).unwrap();
        let mut adapter = MockAdapter {
            handshake: AbiHandshake { major: 2, minor: 0 },
            launched: Vec::new(),
            unhealthy: None,
        };

        assert!(matches!(
            start_ready_services(&mut supervisor, &specs, &mut adapter),
            Err(ServiceBootError::AbiVersionMismatch { .. })
        ));
        assert!(adapter.launched.is_empty());
    }

    #[test]
    fn unhealthy_service_is_not_marked_ready() {
        let specs = vec![spec("security", &[])];
        let mut supervisor = ServiceSupervisor::new(specs.clone()).unwrap();
        let mut adapter = MockAdapter {
            handshake: AbiHandshake::CURRENT,
            launched: Vec::new(),
            unhealthy: Some("security".to_owned()),
        };

        assert!(matches!(
            start_ready_services(&mut supervisor, &specs, &mut adapter),
            Err(ServiceBootError::HealthCheckFailed(_))
        ));
        assert_eq!(supervisor.snapshot("security").unwrap().state, ServiceState::Failed);
    }
}
