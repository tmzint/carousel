use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};

pub struct AssetChangeNotify {
    watcher: RecommendedWatcher,
    receiver: flume::Receiver<AssetChanged>,
}

impl AssetChangeNotify {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let (sender, receiver) = flume::unbounded();
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| {
            match res {
                Ok(event) => {
                    let event: notify::event::Event = event;
                    if event.kind.is_create() || event.kind.is_modify() {
                        for path in event.paths {
                            let _ = sender.send(AssetChanged { path });
                        }
                    }
                }
                Err(e) => {
                    log::error!("asset change notify failed: {}", e);
                }
            };
        })?;

        watcher.configure(notify::Config::PreciseEvents(true))?;

        Ok(Self { watcher, receiver })
    }

    pub(crate) fn watch<P: AsRef<Path>>(&mut self, path: P) -> anyhow::Result<()> {
        self.watcher
            .watch(path.as_ref(), RecursiveMode::Recursive)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn unwatch<P: AsRef<Path>>(&mut self, path: P) -> anyhow::Result<()> {
        self.watcher.unwatch(path.as_ref())?;
        Ok(())
    }

    pub(crate) fn changes_iter(&mut self) -> impl Iterator<Item = AssetChanged> + '_ {
        self.receiver.try_iter()
    }
}

pub(crate) struct AssetChanged {
    pub path: PathBuf,
}
