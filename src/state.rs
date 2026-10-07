//! Plugin-owned registration state: this host's id and the issued client.
//!
//! Gray's credential store holds the tokens and drops them on sign-out or a
//! dead refresh token. The issued `client_id` must outlive that: the next
//! sign-in reauthorizes the same registration instead of registering a new
//! client, and the host id must stay stable for the life of the install.
//! Neither is a secret, but the file is still written owner-only.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration: Option<Registration>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registration {
    /// Issued dynamic client id (`oaiapp_…`), never `dynamic_agent_client`.
    pub client_id: String,
    /// Validated ID-token subject; `None` until the first exchange lands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Display and `login_hint` only — not an identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StateFile {
    path: PathBuf,
}

impl StateFile {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `$GRAY_HOME/chatgpt/state.json`, else `~/.gray/chatgpt/state.json`.
    pub fn default_location() -> Self {
        let home = std::env::var_os("GRAY_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".gray")))
            .unwrap_or_else(|| PathBuf::from(".gray"));
        Self::at(home.join("chatgpt").join("state.json"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Missing or unreadable state is a fresh install, never a crash.
    pub fn load(&self) -> State {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, state: &State) -> std::io::Result<()> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tmp.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        tmp.write_all(&serde_json::to_vec_pretty(state)?)?;
        tmp.as_file().sync_all()?;
        tmp.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }

    /// This install's stable `ext_agent_host_id`, created and persisted on
    /// first use (before the first sign-in, as OpenAI requires).
    pub fn host_id(&self) -> std::io::Result<String> {
        let mut state = self.load();
        if let Some(id) = state.host_id.clone() {
            return Ok(id);
        }
        let id = format!("urn:uuid:{}", uuid::Uuid::new_v4().hyphenated());
        state.host_id = Some(id.clone());
        self.save(&state)?;
        Ok(id)
    }

    pub fn registration(&self) -> Option<Registration> {
        self.load().registration
    }

    pub fn set_registration(&self, registration: Registration) -> std::io::Result<()> {
        let mut state = self.load();
        state.registration = Some(registration);
        self.save(&state)
    }

    /// Forget the registration so the next sign-in registers a new client
    /// (switching ChatGPT accounts or workspaces). The host id stays.
    pub fn forget_registration(&self) -> std::io::Result<()> {
        let mut state = self.load();
        state.registration = None;
        self.save(&state)
    }
}

#[path = "state_tests.rs"]
#[cfg(test)]
mod tests;
