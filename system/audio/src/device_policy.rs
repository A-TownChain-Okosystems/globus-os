//! Audio-device routing and session policy boundary.

use crate::AudioSessionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kind of output a route can target.
pub enum AudioOutputKind {
    /// Built-in speaker.
    Speaker,
    /// Headphone jack.
    Headphones,
    /// Audio over HDMI.
    Hdmi,
    /// USB audio device.
    Usb,
    /// Bluetooth audio device.
    Bluetooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Assignment of one session to one output.
pub struct AudioRoute {
    /// Session that owns the route.
    pub session: AudioSessionId,
    /// Output the session plays on.
    pub output: AudioOutputKind,
    /// True when the session needs the output to itself.
    pub exclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Why a route change was refused.
pub enum RouteError {
    /// The session already has a route.
    DuplicateSession,
    /// The output is held exclusively, or would be.
    ExclusiveConflict,
    /// The session has no route.
    MissingSession,
}

#[derive(Debug, Default)]
/// Routes, kept sorted by session identifier.
pub struct AudioRouteTable {
    routes: Vec<AudioRoute>,
}

impl AudioRouteTable {
    /// Empty table.
    pub fn new() -> Self {
        Self::default()
    }
    /// All routes, sorted by session.
    pub fn routes(&self) -> &[AudioRoute] {
        &self.routes
    }

    /// Adds a route, rejecting duplicates and exclusive conflicts.
    pub fn add(&mut self, route: AudioRoute) -> Result<(), RouteError> {
        if self.routes.iter().any(|r| r.session == route.session) {
            return Err(RouteError::DuplicateSession);
        }
        if route.exclusive && self.routes.iter().any(|r| r.output == route.output) {
            return Err(RouteError::ExclusiveConflict);
        }
        if self
            .routes
            .iter()
            .any(|r| r.output == route.output && r.exclusive)
        {
            return Err(RouteError::ExclusiveConflict);
        }
        self.routes.push(route);
        self.routes.sort_by_key(|r| r.session.0);
        Ok(())
    }

    /// Removes the route of a session.
    pub fn remove(&mut self, session: AudioSessionId) -> Result<(), RouteError> {
        let before = self.routes.len();
        self.routes.retain(|r| r.session != session);
        if self.routes.len() == before {
            return Err(RouteError::MissingSession);
        }
        Ok(())
    }

    /// Route of a session, if it has one.
    pub fn route_for(&self, session: AudioSessionId) -> Option<AudioRoute> {
        self.routes.iter().copied().find(|r| r.session == session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_route_blocks_shared_output() {
        let mut table = AudioRouteTable::new();
        table
            .add(AudioRoute {
                session: AudioSessionId(1),
                output: AudioOutputKind::Hdmi,
                exclusive: true,
            })
            .unwrap();
        assert_eq!(
            table.add(AudioRoute {
                session: AudioSessionId(2),
                output: AudioOutputKind::Hdmi,
                exclusive: false
            }),
            Err(RouteError::ExclusiveConflict)
        );
    }
}
