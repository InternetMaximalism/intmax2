use common::env::EnvType;

use crate::env_var::EnvVar;

use super::error::CliError;

pub fn load_env() -> Result<EnvVar, CliError> {
    let env = envy::from_env::<EnvVar>()?;
    Ok(env)
}

pub fn is_local() -> Result<bool, CliError> {
    Ok(load_env()?.env == EnvType::Local)
}
