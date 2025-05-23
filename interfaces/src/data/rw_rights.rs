use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReadRights {
    AuthRead, // read with authentication
    OpenRead, // read without authentication
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WriteRights {
    SingleAuthWrite, // write once with authentication
    SingleOpenWrite, // write once without authentication
    AuthWrite,       // write multiple times with authentication
    OpenWrite,       // write multiple times without authentication
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RWRights {
    pub read_rights: ReadRights,
    pub write_rights: WriteRights,
}

impl fmt::Display for ReadRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = match self {
            ReadRights::AuthRead => "ra".to_string(),
            ReadRights::OpenRead => "ro".to_string(),
        };
        write!(f, "{t}",)
    }
}

impl FromStr for ReadRights {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ra" => Ok(ReadRights::AuthRead),
            "ro" => Ok(ReadRights::OpenRead),
            _ => Err(format!("Invalid read rights: {s}",)),
        }
    }
}

impl fmt::Display for WriteRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = match self {
            WriteRights::SingleAuthWrite => "swa".to_string(),
            WriteRights::SingleOpenWrite => "swo".to_string(),
            WriteRights::AuthWrite => "wa".to_string(),
            WriteRights::OpenWrite => "wo".to_string(),
        };
        write!(f, "{t}",)
    }
}

impl FromStr for WriteRights {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "swa" => Ok(WriteRights::SingleAuthWrite),
            "swo" => Ok(WriteRights::SingleOpenWrite),
            "wa" => Ok(WriteRights::AuthWrite),
            "wo" => Ok(WriteRights::OpenWrite),
            _ => Err(format!("Invalid write rights: {s}",)),
        }
    }
}

impl fmt::Display for RWRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.read_rights, self.write_rights)
    }
}

impl FromStr for RWRights {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('_').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid rw rights: {s}",));
        }
        let read_rights = ReadRights::from_str(parts[0])?;
        let write_rights = WriteRights::from_str(parts[1])?;
        Ok(RWRights {
            read_rights,
            write_rights,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rw_rights() {
        let rw_rights = RWRights {
            read_rights: ReadRights::AuthRead,
            write_rights: WriteRights::SingleAuthWrite,
        };
        assert_eq!(rw_rights.to_string(), "ra_swa");

        let rw_rights: RWRights = "ra_swa".parse().unwrap();
        assert_eq!(rw_rights.read_rights, ReadRights::AuthRead);
        assert_eq!(rw_rights.write_rights, WriteRights::SingleAuthWrite);
    }
}
