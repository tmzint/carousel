#![feature(map_first_last)]

use relative_path::{RelativePath, RelativePathBuf};
use roundabout::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

mod asset;
mod platform;
pub mod prelude;
mod render;
mod sim;
mod time;
mod util;

#[derive(Debug, Clone)]
pub struct InitEvent {
    pub start: Instant,
    pub dir: PathBuf,
    pub asset_dir: PathBuf,
}

pub struct EngineBuilder {
    dir: PathBuf,
    asset_path: RelativePathBuf,
    runtime: Runtime,
}

impl EngineBuilder {
    fn new() -> anyhow::Result<Self> {
        let current_dir = std::env::current_dir()?;

        Ok(EngineBuilder {
            dir: current_dir,
            asset_path: RelativePath::new("assets/").to_owned(),
            runtime: Runtime::builder(4_194_304).finish(),
        })
    }

    pub fn with_dir<T: Into<PathBuf>>(mut self, dir: T) -> Self {
        self.dir = dir.into();
        self
    }

    pub fn with_asset_path<T: Into<RelativePathBuf>>(mut self, asset_path: T) -> Self {
        self.asset_path = asset_path.into();
        self
    }

    pub fn with_runtime(mut self, runtime: Runtime) -> Self {
        self.runtime = runtime;
        self
    }

    pub fn finish(self) -> Engine {
        let asset_dir = self.asset_path.to_path(&self.dir);
        let init = InitEvent {
            start: Instant::now(),
            dir: self.dir,
            asset_dir,
        };

        Engine {
            init,
            runtime: self.runtime,
        }
    }
}

pub struct Engine {
    init: InitEvent,
    runtime: Runtime,
}

impl Engine {
    pub fn builder() -> anyhow::Result<EngineBuilder> {
        EngineBuilder::new()
    }

    pub fn default(runtime: Runtime) -> anyhow::Result<Self> {
        Ok(Self::builder()?.with_runtime(runtime).finish())
    }

    pub fn start(self) -> anyhow::Result<()> {
        self.runtime.start(self.init);
        Ok(())
    }
}
