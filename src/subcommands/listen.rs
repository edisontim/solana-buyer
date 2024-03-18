use clap::Args;

use crate::{listener::Listener, types::Config};
#[derive(Debug, Args)]
pub struct ListenSubcommand;

impl ListenSubcommand {
    pub fn run(self, config: Config) {
        let listener = Listener::from_config(config);
    }
}
