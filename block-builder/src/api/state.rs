use crate::{
    app::{block_builder::BlockBuilder, error::BlockBuilderError},
    EnvVar,
};

#[derive(Debug, Clone)]
pub struct State {
    pub block_builder: BlockBuilder,
}

impl State {
    pub fn new(env: &EnvVar) -> Result<Self, BlockBuilderError> {
        let block_builder = BlockBuilder::new(env)?;
        Ok(State { block_builder })
    }

    pub fn run(&self) {
        self.block_builder.run();
    }
}
