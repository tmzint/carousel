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
    pub sys_dir: PathBuf,
    pub usr_dir: PathBuf,
}

pub struct EngineBuilder {
    dir: PathBuf,
    sys_path: RelativePathBuf,
    usr_path: RelativePathBuf,
    runtime: Runtime,
}

impl EngineBuilder {
    fn new() -> anyhow::Result<Self> {
        let current_dir = std::env::current_dir()?;

        Ok(EngineBuilder {
            dir: current_dir,
            sys_path: RelativePath::new("sys/").to_owned(),
            usr_path: RelativePath::new("usr/").to_owned(),
            runtime: Runtime::builder(4_194_304).finish(),
        })
    }

    pub fn with_dir<T: Into<PathBuf>>(mut self, dir: T) -> Self {
        self.dir = dir.into();
        self
    }

    pub fn with_sys_path<T: Into<RelativePathBuf>>(mut self, sys_path: T) -> Self {
        self.sys_path = sys_path.into();
        self
    }

    pub fn with_usr_path<T: Into<RelativePathBuf>>(mut self, usr_path: T) -> Self {
        self.usr_path = usr_path.into();
        self
    }

    pub fn with_runtime(mut self, runtime: Runtime) -> Self {
        self.runtime = runtime;
        self
    }

    pub fn finish(self) -> Engine {
        let sys_dir = self.sys_path.to_path(&self.dir);
        let usr_dir = self.usr_path.to_path(&self.dir);

        let init = InitEvent {
            start: Instant::now(),
            dir: self.dir,
            sys_dir,
            usr_dir,
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
