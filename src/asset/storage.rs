use crate::asset::{
    AssetId, AssetIdKind, AssetPath, AssetPathKind, LoadAssetEvent, Loaded, StoreAssetEvent,
    StrongAssetId, SyncQueueEntry, UntypedAsset, UntypedAssetId, WeakAssetId,
};
use crate::prelude::LoadedAssetId;
use crate::util::{HashMap, IndexMap, OrderWindow};
use internment::Intern;
use parking_lot::{RwLock, RwLockReadGuard};
use relative_path::RelativePath;
use roundabout::prelude::{MessageSender, UntypedMessage};
use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct InnerAssets {
    // Optimization: use a custom structure that has better data locality and less indirections (e.g. generational arena based)
    // Optimization: use a smaller key / pre computed hash
    underlying: RwLock<HashMap<UntypedAssetId, UntypedAsset>>,
    counters: RwLock<IndexMap<UntypedAssetId, Arc<()>>>,
    path_id_index: RwLock<BTreeSet<(AssetPath, OrderWindow<UntypedAssetId>)>>,
    unloaded_events: RwLock<HashMap<UntypedAssetId, UntypedMessage>>,
}

#[derive(Clone)]
pub struct Assets {
    pub(crate) inner: Arc<InnerAssets>,
    pub(crate) sender: MessageSender,
    pub(crate) sys_dir: PathBuf,
    pub(crate) usr_dir: PathBuf,
}

impl Assets {
    #[inline]
    pub fn sys_dir(&self) -> &Path {
        &self.sys_dir
    }

    #[inline]
    pub fn usr_dir(&self) -> &Path {
        &self.usr_dir
    }

    #[inline]
    pub fn asset_dir(&self, kind: &AssetPathKind) -> &Path {
        match kind {
            AssetPathKind::Sys => &self.sys_dir,
            AssetPathKind::Usr => &self.usr_dir,
        }
    }

    #[inline]
    pub fn asset_path_kind(&self, path: &Path) -> Option<AssetPathKind> {
        if path.starts_with(&self.sys_dir) {
            return Some(AssetPathKind::Sys);
        }

        if path.starts_with(&self.usr_dir) {
            return Some(AssetPathKind::Usr);
        }

        None
    }

    #[inline]
    pub fn asset_path(&self, path: &Path) -> Option<AssetPath> {
        path.strip_prefix(&self.sys_dir)
            .ok()
            .map(|p| (AssetPathKind::Sys, p))
            .or_else(|| {
                path.strip_prefix(&self.usr_dir)
                    .map(|p| (AssetPathKind::Usr, p))
                    .ok()
            })
            .and_then(|(kind, relative_path_string)| {
                if let Some(relative_path_string) = relative_path_string.to_str() {
                    let path = Intern::new(RelativePath::new(relative_path_string).to_owned());
                    let asset_path = AssetPath::new(kind, path);
                    Some(asset_path)
                } else {
                    None
                }
            })
    }

    #[inline]
    pub fn client(&self) -> AssetsClient {
        AssetsClient {
            underlying: self.inner.underlying.read(),
            counters: &self.inner.counters,
            sender: &self.sender,
        }
    }

    /**
    Safety:
        * the type id of the UntypedAssetId must correspond to the Any instance of the UntypedAsset
    */
    pub(crate) unsafe fn extend<I>(&self, assets: I)
    where
        I: Iterator<Item = SyncQueueEntry>,
    {
        let mut underlying = self.inner.underlying.write();
        let counters = self.inner.counters.write();
        let mut path_id_index = self.inner.path_id_index.write();
        let mut unloaded_events = self.inner.unloaded_events.write();

        for entry in assets {
            let count = counters
                .get(&entry.asset_id)
                .map(Arc::strong_count)
                .unwrap_or_default();
            if count > 0 {
                underlying.insert(entry.asset_id, entry.asset);

                if let Some(asset_path) = entry.asset_id.kind.asset_path() {
                    path_id_index.insert((asset_path, OrderWindow::new(entry.asset_id)));
                }
                if let Some(loaded_event) = entry.loaded_event {
                    self.sender.send_untyped(loaded_event);
                }
                if let Some(unloaded_event) = entry.unloaded_event {
                    unloaded_events.insert(entry.asset_id, unloaded_event);
                }
            }
        }
    }

    pub(crate) fn gc(&self, at: usize, max: usize) -> usize {
        let (gc_assets, next) = {
            let counters = self.inner.counters.read();
            let next = at.saturating_add(max).max(counters.len()) % counters.len();
            let gc_assets: Vec<UntypedAssetId> = counters
                .iter()
                .skip(at)
                .take(max)
                .filter_map(|(k, c)| {
                    if Arc::strong_count(c) < 2 {
                        Some(*k)
                    } else {
                        None
                    }
                })
                .collect();

            (gc_assets, next)
        };

        if gc_assets.is_empty() {
            return next;
        }

        let mut underlying = self.inner.underlying.write();
        let mut counters = self.inner.counters.write();
        let mut path_id_index = self.inner.path_id_index.write();
        let mut unloaded_events = self.inner.unloaded_events.write();

        for gc_asset in gc_assets {
            let counts = counters
                .get(&gc_asset)
                .map(Arc::strong_count)
                .unwrap_or_default();

            if counts < 2 {
                log::info!("unloading asset: {:?}", gc_asset);
                counters.remove(&gc_asset);
                underlying.remove(&gc_asset);
                if let Some(asset_path) = gc_asset.kind.asset_path() {
                    path_id_index.remove(&(asset_path, OrderWindow::new(gc_asset)));
                }
                if let Some(unloaded_event) = unloaded_events.remove(&gc_asset) {
                    self.sender.send_untyped(unloaded_event);
                }
            }
        }

        next
    }

    pub(crate) fn asset_ids_for_path(&self, asset_path: AssetPath) -> Vec<UntypedAssetId> {
        use std::ops::Bound::Included;
        let path_id_index = self.inner.path_id_index.read();
        path_id_index
            .range((
                Included(&(asset_path, OrderWindow::Start)),
                Included(&(asset_path, OrderWindow::End)),
            ))
            .flat_map(|(_, id)| id.as_option().copied())
            .collect()
    }
}

pub struct AssetsClient<'a> {
    underlying: RwLockReadGuard<'a, HashMap<UntypedAssetId, UntypedAsset>>,
    counters: &'a RwLock<IndexMap<UntypedAssetId, Arc<()>>>,
    sender: &'a MessageSender,
}

impl<'a> AssetsClient<'a> {
    #[inline]
    pub fn load<T: Send + Sync + 'static>(&self, asset_path: AssetPath) -> StrongAssetId<T> {
        let weak = WeakAssetId::new(AssetIdKind::AssetPath(asset_path));

        // have to use a val as a direct match won't drop the read lock
        let counter = self.counters.read().get(&weak.untyped).cloned();
        match counter {
            Some(counter) => weak.into_strong(counter),
            None => {
                let counter = Arc::new(());
                self.counters.write().insert(weak.untyped, counter.clone());

                log::info!("queue load asset: {:?}", weak);
                if !self.sender.borrow().send(LoadAssetEvent::new(weak, false)) {
                    panic!(
                        "missing asset loader for the asset type of {}",
                        std::any::type_name::<T>()
                    )
                }

                weak.into_strong(counter)
            }
        }
    }

    #[inline]
    pub fn try_upgrade<T: Send + Sync + 'static>(
        &self,
        weak: &WeakAssetId<T>,
    ) -> Option<StrongAssetId<T>> {
        match weak.untyped.kind {
            AssetIdKind::AssetPath(path) => Some(self.load(path)),
            AssetIdKind::Uuid(_) => self
                .counters
                .read()
                .get(&weak.untyped)
                .map(|c| weak.into_strong(c.clone())),
        }
    }

    #[inline]
    pub fn try_loaded<T: Send + Sync + 'static>(
        &self,
        strong: &StrongAssetId<T>,
    ) -> Option<LoadedAssetId<T>> {
        unsafe {
            if self.has(strong) {
                Some(strong.to_owned().into_loaded())
            } else {
                None
            }
        }
    }

    // TODO: support uuid?
    #[inline]
    pub fn store<T: Send + Sync + 'static, AI: Into<WeakAssetId<T>>>(
        &self,
        asset_id: AI,
        asset: T,
    ) -> StrongAssetId<T> {
        let asset_id = asset_id.into();

        // have to use a val as a direct match won't drop the read lock
        let counter = self.counters.read().get(&asset_id.untyped).cloned();
        let strong_asset_id = match counter {
            Some(counter) => asset_id.into_strong(counter),
            None => {
                let counter = Arc::new(());
                self.counters
                    .write()
                    .insert(asset_id.untyped, counter.clone());

                asset_id.into_strong(counter)
            }
        };

        log::info!("queue store asset: {:?}", asset_id);
        let sender = self.sender.borrow();
        assert!(sender.send(StoreAssetEvent::new(asset_id, asset, sender)));

        strong_asset_id
    }

    #[inline]
    pub fn has<T, S>(&self, id: &AssetId<T, S>) -> bool {
        self.underlying.get(&id.untyped).is_some()
    }

    #[inline]
    pub fn get<T: std::any::Any>(&self, id: &AssetId<T, Loaded>) -> &T {
        let t = self.underlying.get(&id.untyped).unwrap();
        // see std::any::Any::downcast_ref()
        // the type check was already done via the typed asset id
        let any: &(dyn std::any::Any + Send + Sync) = t.as_ref();
        unsafe { &*(any as *const dyn std::any::Any as *const T) }
    }

    #[inline]
    pub fn try_get<T: std::any::Any, S>(&self, id: &AssetId<T, S>) -> Option<&T> {
        self.underlying.get(&id.untyped).map(|t| {
            // see std::any::Any::downcast_ref()
            // the type check was already done via the typed asset id
            let any: &(dyn std::any::Any + Send + Sync) = t.as_ref();
            unsafe { &*(any as *const dyn std::any::Any as *const T) }
        })
    }
}
