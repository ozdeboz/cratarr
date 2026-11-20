//! Play On as Cratarr's own source asks it (`playon_base::Players`).

use std::future::Future;
use std::pin::Pin;

use super::ToggleError;
use super::playon::PlayOn;
use super::playon_base::{Command, PlayerView, Players};

impl Players for PlayOn {
    fn get(&self, name: &str) -> Option<PlayerView> {
        PlayOn::get(self, name)
    }

    fn followed(&self) -> Option<PlayerView> {
        PlayOn::followed(self)
    }

    fn summary(&self) -> String {
        PlayOn::summary(self)
    }

    fn command<'a>(&'a self, name: &'a str, command: Command) -> Pin<Box<dyn Future<Output = Result<(), ToggleError>> + Send + 'a>> {
        Box::pin(PlayOn::command(self, name, command))
    }
}
