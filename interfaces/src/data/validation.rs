pub trait Validation {
    fn validate(&self) -> anyhow::Result<()>;
}
