use aws_config::SdkConfig;
use aws_sdk_s3::{error::SdkError, presigning::PresigningConfig, Client as AwsS3Client};
use base64::{prelude::BASE64_STANDARD, Engine};
use serde::Deserialize;
use std::{io, time::Duration};

pub type Result<T> = std::result::Result<T, S3Error>;

#[derive(Debug, Clone, Deserialize)]
pub struct S3Config {
    pub bucket_name: String,
    pub cloudfront_domain: String,
    pub cloudfront_key_pair_id: String,
    pub cloudfront_private_key_base64: String,
}

#[derive(Clone)]
pub struct S3Client {
    client: AwsS3Client,
    config: S3Config,
}

impl S3Client {
    pub fn new(aws_config: SdkConfig, config: S3Config) -> Self {
        let client = AwsS3Client::new(&aws_config);
        Self { client, config }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum S3Error {
    #[error("Failed to create presigning configuration: {0}")]
    PresigningConfig(String),

    #[error("Failed to parse private key: {0}")]
    ParsePrivateKeyError(String),

    #[error("Failed to generate presigned upload URL: {0}")]
    PresignedUrlGeneration(String),

    #[error("Failed to check if object exists: {0}")]
    ObjectExistenceCheck(String),

    #[error("Failed to read private key file: {0}")]
    IoError(#[from] io::Error),

    #[error("Failed to generate CloudFront signed URL: {0}")]
    CloudFrontSigning(String),
}

impl S3Client {
    pub async fn generate_upload_url(
        &self,
        key: &str,
        content_type: &str,
        expiration: Duration,
    ) -> Result<String> {
        let presigning_config = PresigningConfig::builder()
            .expires_in(expiration)
            .build()
            .map_err(|e| S3Error::PresigningConfig(e.to_string()))?;

        let presigned_request = self
            .client
            .put_object()
            .bucket(&self.config.bucket_name)
            .key(key)
            .content_type(content_type)
            .presigned(presigning_config)
            .await
            .map_err(|e| S3Error::PresignedUrlGeneration(e.to_string()))?;

        Ok(presigned_request.uri().to_string())
    }

    pub fn generate_download_url(
        &self,
        resource_path: &str,
        expiration: Duration,
    ) -> Result<String> {
        let url = format!(
            "https://{}/{}",
            self.config.cloudfront_domain, resource_path
        );

        let private_key_bytes = BASE64_STANDARD
            .decode(&self.config.cloudfront_private_key_base64)
            .map_err(|e| S3Error::ParsePrivateKeyError(format!("failed to decode base64: {e}")))?;
        let private_key = String::from_utf8(private_key_bytes)
            .map_err(|e| S3Error::ParsePrivateKeyError(format!("failed to parse utf8: {e}")))?;

        let options = cloudfront_sign::SignedOptions {
            key_pair_id: self.config.cloudfront_key_pair_id.clone().into(),
            private_key: private_key.into(),
            date_less_than: chrono::Utc::now().timestamp() as u64 + expiration.as_secs(),
            ..Default::default()
        };

        let signed_url = cloudfront_sign::get_signed_url(&url, &options)
            .map_err(|e| S3Error::CloudFrontSigning(format!("{e:?}")))?;

        Ok(signed_url)
    }

    pub async fn check_object_exists(&self, key: &str) -> Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.config.bucket_name)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(err) => {
                if let SdkError::ServiceError(service_err) = &err {
                    if service_err.err().is_not_found() {
                        return Ok(false);
                    }
                }
                Err(S3Error::ObjectExistenceCheck(format!("{err:?}")))
            }
        }
    }

    pub async fn delete_object(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.config.bucket_name)
            .key(key)
            .send()
            .await
            .map_err(|e| S3Error::ObjectExistenceCheck(format!("{e:?}")))?;
        Ok(())
    }
}

#[cfg(test)]
mockall::mock! {
    pub S3Client {
        pub fn new(aws_config: SdkConfig, config: S3Config) -> Self;

        pub async fn generate_upload_url(
            &self,
            key: &str,
            content_type: &str,
            expiration: Duration,
        ) -> Result<String>;

        pub fn generate_download_url(
            &self,
            resource_path: &str,
            expiration: Duration,
        ) -> Result<String>;

        pub async fn check_object_exists(&self, key: &str) -> Result<bool>;

        pub async fn delete_object(&self, key: &str) -> Result<()>;
    }

    impl Clone for S3Client {
        fn clone(&self) -> Self;
    }
}
