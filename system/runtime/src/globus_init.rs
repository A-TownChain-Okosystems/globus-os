//! GlobusOS init entry point for platform-provided service process adapters.
//!
//! This is the single userspace orchestration entry point for dependency ordering,
//! ABI compatibility, capability provisioning, launch, and readiness checks. The
//! actual platform adapter must call the supported ShivaCore process syscall path;
//! the init layer does not simulate successful launches itself.

use globus_services::{ServiceSpec, ServiceSupervisor, SupervisorError};
use crate::service_boot::{start_ready_services, ServiceBootError, ServiceProcessAdapter, StartedService};

#[derive(Debug, PartialEq, Eq)]
pub enum GlobusInitError {
    InvalidBootPlan,
    Supervisor(SupervisorError),
    Boot(ServiceBootError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    /// Services which passed both launch and readiness checks, in startup order.
    pub ready_services: Vec<StartedService>,
}

/// Run the GlobusOS service init sequence using a real platform process adapter.
///
/// The function returns only services which have passed the adapter's health
/// check. An adapter error is propagated; no service is reported as ready merely
/// because a launch request was issued.
pub fn run_globus_init(
    specs: &[ServiceSpec],
    adapter: &mut impl ServiceProcessAdapter,
) -> Result<InitReport, GlobusInitError> {
    if !globus_system_core::validate_boot_plan() {
        return Err(GlobusInitError::InvalidBootPlan);
    }
    let mut supervisor =
        ServiceSupervisor::new(specs.to_vec()).map_err(GlobusInitError::Supervisor)?;
    let ready_services = start_ready_services(&mut supervisor, specs, adapter)
        .map_err(GlobusInitError::Boot)?;
    Ok(InitReport { ready_services })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_boot::ServiceProcessAdapter;
    use libshivacore::{AbiHandshake, CapabilityHandle};

    struct TestAdapter {
        launched: Vec<String>,
    }

    impl ServiceProcessAdapter for TestAdapter {
        fn abi_handshake(&mut self) -> Result<AbiHandshake, String> {
            Ok(AbiHandshake::CURRENT)
        }

        fn launch(
            &mut self,
            service: &ServiceSpec,
        ) -> Result<libshivacore::ProcessId, String> {
            self.launched.push(service.name.clone());
            Ok(libshivacore::ProcessId(self.launched.len() as u32 + 1000))
        }

        fn initial_capabilities(
            &mut self,
            _service: &str,
            _process: libshivacore::ProcessId,
        ) -> Result<Vec<CapabilityHandle>, String> {
            Ok(Vec::new())
        }

        fn activate(
            &mut self,
            _service: &ServiceSpec,
            _process: libshivacore::ProcessId,
            _capabilities: &[CapabilityHandle],
        ) -> Result<(), String> {
            Ok(())
        }

        fn is_healthy(
            &mut self,
            _service: &ServiceSpec,
            _process: libshivacore::ProcessId,
        ) -> Result<bool, String> {
            Ok(true)
        }

        fn terminate(
            &mut self,
            _service: &ServiceSpec,
            _process: libshivacore::ProcessId,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn init_entrypoint_returns_only_health_checked_services() {
        let specs = vec![
            ServiceSpec {
                name: "ipc".to_owned(),
                dependencies: vec![],
                critical: true,
            },
            ServiceSpec {
                name: "globusd".to_owned(),
                dependencies: vec!["ipc".to_owned()],
                critical: true,
            },
        ];
        let mut adapter = TestAdapter { launched: Vec::new() };
        let report = run_globus_init(&specs, &mut adapter).unwrap();
        assert_eq!(
            report.ready_services.iter().map(|service| service.name.as_str()).collect::<Vec<_>>(),
            vec!["ipc", "globusd"]
        );
        assert_eq!(
            adapter.launched,
            report.ready_services.iter().map(|service| service.name.clone()).collect::<Vec<_>>()
        );
    }
}
