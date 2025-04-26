use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EnvType {
    Local,
    Dev,
    Staging,
    Prod,
}

pub fn get_env_type() -> EnvType {
    match std::env::var("ENV") {
        Ok(env) => match env.as_str() {
            "local" => EnvType::Local,
            "dev" => EnvType::Dev,
            "staging" => EnvType::Staging,
            "prod" => EnvType::Prod,
            _ => panic!("Invalid ENV value"),
        },
        // default to Dev if ENV is not set
        Err(_) => EnvType::Dev,
    }
}
