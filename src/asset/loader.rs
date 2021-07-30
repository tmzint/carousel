use crate::asset::storage::{Assets, AssetsClient, RegisterAssetResult};
use crate::asset::{
    AssetId, AssetPath, AssetUri, AssetUriVisitor, DependencyQueueEntry, Loaded, LoadedAssetId,
    Strong, StrongAssetId, SyncQueueEntry, Weak, WeakAssetId,
};
use crate::util::IndexMap;
use internment::Intern;
use relative_path::RelativePath;
use relative_path::RelativePathBuf;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::DerefMut;

pub struct AssetCursorChildren<'a, 'b> {
    cursor: &'b mut AssetCursor<'a>,
    children: Vec<AssetPath>,
}

impl<'a, 'b> AssetCursorChildren<'a, 'b> {
    #[inline]
    pub fn next(&mut self) -> Option<AssetCursor> {
        self.children.pop().map(move |ap| AssetCursor {
            asset_path: ap,
            assets: self.cursor.assets,
            sync_queue: self.cursor.sync_queue,
            dependency_queue: self.cursor.dependency_queue,
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.children.len()
    }
}

pub struct AssetCursor<'a> {
    pub(crate) asset_path: AssetPath,
    pub(crate) assets: &'a mut Assets,
    pub(crate) sync_queue: &'a mut Vec<SyncQueueEntry>,
    pub(crate) dependency_queue: &'a mut Vec<DependencyQueueEntry>,
}

impl<'a> AssetCursor<'a> {
    #[inline]
    pub fn asset_path(&self) -> &AssetPath {
        &self.asset_path
    }

    #[inline]
    pub fn read(&self) -> anyhow::Result<Vec<u8>> {
        let path = self
            .asset_path
            .path
            .to_path(self.assets.paths.asset_dir(&self.asset_path.kind));
        log::info!("reading asset from: {}", path.display());
        let bytes = std::fs::read(&path)?;
        Ok(bytes)
    }

    #[inline]
    pub fn extension(&self) -> Option<&str> {
        self.asset_path.path.extension()
    }

    #[inline]
    pub fn children<'b>(&'b mut self) -> anyhow::Result<AssetCursorChildren<'a, 'b>> {
        let mut paths = Vec::new();

        let path = self
            .asset_path
            .path
            .to_path(self.assets.paths.asset_dir(&self.asset_path.kind));
        let dir = std::fs::read_dir(path)?;
        for entry in dir {
            let entry = entry?;
            let entry_path = entry.path();

            // TODO: configurable filter (glob?), also see lower entry_path.file_name()
            if !entry_path.is_file() {
                continue;
            }

            if let Some(file_name) = entry_path.file_name().and_then(|s| s.to_str()) {
                let entry_rel_path = Intern::new(self.asset_path.path.join(file_name));
                paths.push(AssetPath::new(self.asset_path.kind, entry_rel_path));
            }
        }

        paths.reverse();

        Ok(AssetCursorChildren {
            cursor: self,
            children: paths,
        })
    }

    #[inline]
    pub fn queue_store<T: 'static + Send + Sync>(
        &mut self,
        id: WeakAssetId<T>,
        asset: T,
    ) -> StrongAssetId<T> {
        self.sync_queue
            .push(SyncQueueEntry::new(id, asset, &self.assets.sender));

        match self.assets.client().register_asset(&id) {
            RegisterAssetResult::Preexisting(id) => id,
            RegisterAssetResult::Unfamiliar(id) => id,
        }
    }

    // TODO: force
    #[inline]
    pub fn queue_load<T: 'static + Send + Sync>(
        &mut self,
        asset_path: AssetPath,
    ) -> StrongAssetId<T> {
        // on changes see SerdeThreadLocal usage
        let weak = WeakAssetId::new(AssetUri::AssetPath(asset_path));
        match self.assets.client().register_asset(&weak) {
            RegisterAssetResult::Preexisting(id) => id,
            RegisterAssetResult::Unfamiliar(id) => {
                self.dependency_queue.push(DependencyQueueEntry {
                    asset_id: id.untyped,
                    force: false,
                });

                id
            }
        }
    }

    #[inline]
    pub unsafe fn queue_store_optimistic<T: 'static + Send + Sync>(
        &mut self,
        id: WeakAssetId<T>,
        asset: T,
    ) -> LoadedAssetId<T> {
        self.queue_store(id, asset).into_loaded()
    }

    #[inline]
    pub unsafe fn queue_load_optimistic<T: 'static + Send + Sync>(
        &mut self,
        asset_path: AssetPath,
    ) -> LoadedAssetId<T> {
        self.queue_load(asset_path).into_loaded()
    }
}

pub trait AssetLoader: Sized + Send + Sync + 'static {
    type Asset: Sized + Send + Sync + 'static;

    fn load(cursor: &mut AssetCursor) -> anyhow::Result<Self::Asset>;
}

// Required to provide serde deserializer context for recursive asset loading
thread_local!(static SERDE_THREAD_LOCAL: RefCell<Option<SerdeThreadLocal>> = RefCell::new(None));

struct SerdeThreadLocal {
    assets: Assets,
    dependency_queue: Vec<DependencyQueueEntry>,
}

pub struct SerdeAssetLoader<T> {
    _pd: PhantomData<T>,
}

impl<T: DeserializeOwned + Send + Sync + 'static> AssetLoader for SerdeAssetLoader<T> {
    type Asset = T;

    #[inline]
    fn load(cursor: &mut AssetCursor) -> anyhow::Result<Self::Asset> {
        SERDE_THREAD_LOCAL.with(|stl| {
            *stl.borrow_mut() = Some(SerdeThreadLocal {
                assets: cursor.assets.to_owned(),
                dependency_queue: Vec::default(),
            });

            let extension = cursor.extension().ok_or_else(|| {
                anyhow::anyhow!(
                    "could not derive file type for serde asset loader: {}",
                    cursor.asset_path
                )
            })?;

            let asset = match extension {
                "json" => serde_json::from_slice(&cursor.read()?)?,
                s => Err(anyhow::anyhow!(
                    "unhandled file type for serde asset loader: {}",
                    s
                ))?,
            };

            cursor.dependency_queue.extend(
                stl.borrow_mut()
                    .as_mut()
                    .unwrap()
                    .dependency_queue
                    .drain(..),
            );

            Ok(asset)
        })
    }
}

impl<'de, T: Send + Sync + 'static> Deserialize<'de> for AssetId<T, Weak> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<WeakAssetId<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let uri = deserializer.deserialize_string(AssetUriVisitor)?;
        Ok(WeakAssetId::new(uri))
    }
}

impl<'de, T: Send + Sync + 'static> Deserialize<'de> for AssetId<T, Strong> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<StrongAssetId<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // TODO: also allow inlined assets (inline vs embedded?)
        //  * uuid(even needed? only if referenced multiple times? serialization vs deserialization ... -> that's not inline but embedded?)
        //  * data for T => insert into Assets (check deadlocks)
        //  How? the api expects Vec<u8>
        //  "SceneFiles" that have a main Asset that reference other embedded/inlined assets
        //  AssetIds need to define if they are external/embedded/inlined

        let uri = deserializer.deserialize_string(AssetUriVisitor)?;
        let weak: WeakAssetId<T> = WeakAssetId::new(uri);

        SERDE_THREAD_LOCAL.with(|maybe_tls| {
            let mut borrow_maybe_tls = maybe_tls.borrow_mut();
            match borrow_maybe_tls.deref_mut() {
                Some(tls) => {
                    let strong = match tls.assets.client().register_asset(&weak) {
                        RegisterAssetResult::Preexisting(id) => id,
                        RegisterAssetResult::Unfamiliar(id) => {
                            tls.dependency_queue.push(DependencyQueueEntry {
                                asset_id: id.untyped,
                                force: false,
                            });

                            id
                        }
                    };

                    Ok(strong)
                }
                None => Err(serde::de::Error::custom(
                    "strong/loaded asset ids can only be deserialized by the asset server",
                )),
            }
        })
    }
}

impl<'de, T: Send + Sync + 'static> Deserialize<'de> for AssetId<T, Loaded> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<LoadedAssetId<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        unsafe {
            let strong = AssetId::<T, Strong>::deserialize(deserializer)?;
            if let AssetUri::Uuid(_) = strong.untyped.uri {
                return Err(serde::de::Error::custom(
                    "loaded asset ids can only be deserialized when the uri is not an uuid",
                ));
            }

            Ok(strong.into_loaded())
        }
    }
}

impl<T: Send + Sync + 'static, TS> Serialize for AssetId<T, TS> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.untyped.uri.to_string())
    }
}

pub type WeakAssetTable<T> = AssetTable<T, Weak>;
pub type StrongAssetTable<T> = AssetTable<T, Strong>;
pub type LoadedAssetTable<T> = AssetTable<T, Loaded>;

pub trait AssetTableKey {
    fn key(self) -> Intern<RelativePathBuf>;
}

impl AssetTableKey for Intern<RelativePathBuf> {
    fn key(self) -> Intern<RelativePathBuf> {
        self
    }
}

impl AssetTableKey for &Intern<RelativePathBuf> {
    fn key(self) -> Intern<RelativePathBuf> {
        *self
    }
}

impl<'a> AssetTableKey for &'a RelativePath {
    fn key(self) -> Intern<RelativePathBuf> {
        Intern::new(self.to_owned())
    }
}

impl AssetTableKey for RelativePathBuf {
    fn key(self) -> Intern<RelativePathBuf> {
        Intern::new(self)
    }
}

impl<'a> AssetTableKey for &'a str {
    fn key(self) -> Intern<RelativePathBuf> {
        Intern::new(RelativePath::new(self).to_owned())
    }
}

impl<'a> AssetTableKey for &'a String {
    fn key(self) -> Intern<RelativePathBuf> {
        Intern::new(RelativePath::new(self).to_owned())
    }
}

#[derive(Debug)]
pub struct AssetTable<T: 'static, S>(IndexMap<Intern<RelativePathBuf>, AssetId<T, S>>);

impl<T: Send + Sync + 'static, S> AssetTable<T, S> {
    pub fn get<K: AssetTableKey>(&self, key: K) -> Option<&AssetId<T, S>> {
        self.0.get(&key.key())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Intern<RelativePathBuf>, &AssetId<T, S>)> {
        self.0.iter()
    }
}

impl<T: Send + Sync + 'static> AssetTable<T, Weak> {
    pub fn upgrade(&self, client: &AssetsClient) -> StrongAssetTable<T> {
        let mut strong_table = IndexMap::default();

        for (k, v) in &self.0 {
            let strong = client.upgrade(v);
            strong_table.insert(*k, strong);
        }

        AssetTable(strong_table)
    }
}

impl<T: Send + Sync + 'static> AssetTable<T, Strong> {
    pub fn try_loaded(&self, client: &AssetsClient) -> Option<LoadedAssetTable<T>> {
        let mut loaded_table = IndexMap::default();

        for (k, v) in &self.0 {
            match client.try_loaded(v) {
                Some(loaded) => {
                    loaded_table.insert(*k, loaded);
                }
                None => {
                    return None;
                }
            }
        }

        Some(AssetTable(loaded_table))
    }
}

pub struct AssetTableLoader<T, S> {
    _pd_t: PhantomData<T>,
    _pd_s: PhantomData<S>,
}

impl<T: Send + Sync + 'static> AssetLoader for AssetTableLoader<T, Weak> {
    type Asset = AssetTable<T, Weak>;

    fn load(cursor: &mut AssetCursor) -> anyhow::Result<Self::Asset> {
        let mut underlying = IndexMap::default();

        let mut children = cursor.children()?;
        while let Some(child) = children.next() {
            underlying.insert(
                child.asset_path.path,
                WeakAssetId::new(AssetUri::AssetPath(child.asset_path)),
            );
        }

        Ok(AssetTable(underlying))
    }
}

impl<T: Send + Sync + 'static> AssetLoader for AssetTableLoader<T, Strong> {
    type Asset = AssetTable<T, Strong>;

    #[inline]
    fn load(cursor: &mut AssetCursor) -> anyhow::Result<Self::Asset> {
        let mut underlying = IndexMap::default();

        let mut children = cursor.children()?;
        while let Some(mut child) = children.next() {
            let strong: StrongAssetId<T> = child.queue_load(child.asset_path);
            underlying.insert(child.asset_path.path, strong);
        }

        Ok(AssetTable(underlying))
    }
}

impl<T: Send + Sync + 'static> AssetLoader for AssetTableLoader<T, Loaded> {
    type Asset = AssetTable<T, Loaded>;

    #[inline]
    fn load(cursor: &mut AssetCursor) -> anyhow::Result<Self::Asset> {
        unsafe {
            let mut underlying = IndexMap::default();

            let mut children = cursor.children()?;
            while let Some(mut child) = children.next() {
                let loaded: LoadedAssetId<T> = child.queue_load_optimistic(child.asset_path);
                underlying.insert(child.asset_path.path, loaded);
            }

            Ok(AssetTable(underlying))
        }
    }
}
