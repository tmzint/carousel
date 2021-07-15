mod state;

use crate::state::{SimResource, State};
use carousel::prelude::*;

pub const LOGICAL_WIDTH: u32 = 720;
pub const LOGICAL_HEIGHT: u32 = 720;

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .filter_module("wgpu_core", log::LevelFilter::Warn)
        .init();

    let runtime = Runtime::builder(2_097_152)
        .add(RenderServer::new)
        .add(TimeServer::new)
        .add(|b| AssetServer::builder(b).finish())
        .add_group(|mut g| {
            let resource_init = SimResource::init_fn(&mut g);
            SimServer::builder(g, resource_init).init_fn(State::new)
        })
        .finish_main_group(|g| PlatformServer::new("display.json", "actions.json", g));

    Engine::builder()?
        .with_asset_path("carousel/examples/serpent/assets/")
        .with_runtime(runtime)
        .finish()
        .start()
}
