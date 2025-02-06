use crate::{app::block_builder::BlockBuilder, EnvVar};

#[derive(Debug, Clone)]
pub struct State {
    pub block_builder: BlockBuilder,
}

impl State {
    pub fn new(env: &EnvVar) -> Self {
        let block_builder = BlockBuilder::new(env);
        State { block_builder }
    }

    pub fn run(&self) {
        self.block_builder.run();
    }
}
