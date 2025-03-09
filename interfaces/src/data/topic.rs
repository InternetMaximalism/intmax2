use crate::data::rw_rights::RWRights;

use super::rw_rights::{ReadRights, WriteRights};

/// Generate a topic used in store-vault for a given set of read and write rights.
pub fn topic_from_rights(read_rights: ReadRights, write_rights: WriteRights, name: &str) -> String {
    let rw_rights = RWRights {
        read_rights,
        write_rights,
    };
    format!("v1/{}/{}", rw_rights, name)
}
