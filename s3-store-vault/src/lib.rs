use serde::Deserialize;

pub mod api;
pub mod app;

#[derive(Deserialize)]
pub struct EnvVar {
    pub port: u16,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_timeout: u64,

    // S3 config
    pub bucket_name: String,
    pub cloudfront_domain: String,
    pub cloudfront_key_pair_id: String,
    pub cloudfront_private_key_base64: String,

    pub s3_upload_timeout: u64,
    pub s3_download_timeout: u64,
}
