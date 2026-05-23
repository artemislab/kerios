//! Config source adapters — pull layered config bundles from external systems.

use crate::merge::ConfigLayer;

pub mod git;

/// Contract for a config source: fetch the bundle, then expose each
/// layer (`org`, `teams/<name>`, `users/<name>`) as a [`ConfigLayer`].
///
/// Each implementation decides its own error type to keep transport-specific
/// failures rich, while the daemon code stays generic over `ConfigSource`.
pub trait ConfigSource {
    /// Transport-specific error type. `Send + Sync` so a `SyncReport`
    /// produced from any source can flow across `tokio::spawn_blocking`
    /// boundaries and `anyhow`-wrapped error chains.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Refresh the local state from the upstream (clone/pull, GET, etc.).
    ///
    /// # Errors
    /// Returns the transport's error if the refresh fails.
    fn fetch(&self) -> Result<(), Self::Error>;

    /// Read the org-wide layer.
    ///
    /// # Errors
    /// Returns the transport's error if the read fails. Returns an empty layer
    /// (not an error) when the org section is absent.
    fn read_org_layer(&self) -> Result<ConfigLayer, Self::Error>;

    /// Read the team-scoped layer for `team`.
    ///
    /// # Errors
    /// Returns the transport's error if the read fails. Returns an empty layer
    /// (not an error) when the team section is absent.
    fn read_team_layer(&self, team: &str) -> Result<ConfigLayer, Self::Error>;

    /// Read the user-scoped layer for `user`.
    ///
    /// # Errors
    /// Returns the transport's error if the read fails. Returns an empty layer
    /// (not an error) when the user section is absent.
    fn read_user_layer(&self, user: &str) -> Result<ConfigLayer, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::git::GitSource;

    /// Compile-time + runtime proof that `GitSource` satisfies the
    /// `ConfigSource` trait. This is what unblocks generic daemon code
    /// like `fn sync_loop<S: ConfigSource>(s: &S)` in later stories.
    #[test]
    fn git_source_satisfies_config_source_trait() {
        fn assert_impl<S: ConfigSource>(_: &S) {}
        let source = GitSource::new("file:///dev/null".to_string(), "/dev/null".into());
        assert_impl(&source);
    }
}
