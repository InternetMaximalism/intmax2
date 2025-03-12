use crate::data::rw_rights::RWRights;

/// Generate a topic used in store-vault for a given set of read and write rights.
pub fn topic_from_rights(rw_rights: RWRights, name: &str) -> String {
    format!("v1/{}/{}", rw_rights, name)
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ExtractRightsError {
    #[error("Invalid path")]
    InvalidPath,

    #[error("Invalid version")]
    InvalidVersion,

    #[error("rights parse error: {0}")]
    RightsParseError(String),
}

pub fn extract_rights(topic: &str) -> Result<RWRights, ExtractRightsError> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() < 2 {
        return Err(ExtractRightsError::InvalidPath);
    }
    let version_part = parts[0];
    if version_part != "v1" {
        return Err(ExtractRightsError::InvalidVersion);
    }
    let rw_rights_part = parts[1];

    let rw_rights: RWRights = rw_rights_part
        .parse()
        .map_err(ExtractRightsError::RightsParseError)?;
    Ok(rw_rights)
}
