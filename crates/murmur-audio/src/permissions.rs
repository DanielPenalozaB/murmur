use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::AudioError;

/// Whether macOS lets Murmur capture a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Granted,
    Denied,
    /// Not asked yet; macOS asks the first time a recording captures it.
    Undetermined,
    /// macOS didn't say (an older or newer version than this was written for).
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Permissions {
    pub mic: Access,
    pub system: Access,
}

impl Permissions {
    /// Parses the helper's `mic=<state> system=<state>` line.
    pub fn parse(line: &str) -> Self {
        let mut permissions = Permissions {
            mic: Access::Unknown,
            system: Access::Unknown,
        };
        for pair in line.split_whitespace() {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            let access = match value {
                "granted" => Access::Granted,
                "denied" => Access::Denied,
                "undetermined" => Access::Undetermined,
                _ => Access::Unknown,
            };
            match key {
                "mic" => permissions.mic = access,
                "system" => permissions.system = access,
                _ => {}
            }
        }
        permissions
    }

    /// Asks the `audiocap` helper, whose access macOS attributes to the app.
    pub fn check(helper: &Path) -> Result<Self, AudioError> {
        let output = Command::new(helper).arg("--permissions").output()?;
        Ok(Self::parse(&String::from_utf8_lossy(&output.stdout)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_helper_line() {
        assert_eq!(
            Permissions::parse("mic=granted system=denied\n"),
            Permissions {
                mic: Access::Granted,
                system: Access::Denied
            }
        );
        assert_eq!(
            Permissions::parse("system=undetermined mic=weird extra"),
            Permissions {
                mic: Access::Unknown,
                system: Access::Undetermined
            }
        );
        assert_eq!(Permissions::parse("").mic, Access::Unknown);
    }
}
