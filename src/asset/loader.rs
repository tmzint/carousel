use crate::asset::storage::{Assets, AssetsClient};
use crate::asset::{
    AssetId, AssetPath, AssetUri, AssetUriVisitor, Loaded, Strong, StrongAssetId, Weak, WeakAssetId,
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

pub struct AssetCursorChild<'a, 'b, 'c> {
    children: &'c mut AssetCursorChildren<'a, 'b>,
    asset_path: AssetPath,
}

impl<'a, 'b, 'c> AssetCursorChild<'a, 'b, 'c> {
    #[inline]
    pub fn asset_path(&self) -> &AssetPath {
        &self.asset_path
    }

    #[inline]
    pub fn read(&self) -> anyhow::Result<Vec<u8>> {
        let path = self.asset_path.path.to_path(
            self.children
                .cursor
                .assets
                .paths
                .asset_dir(&self.asset_path.kind),
        );
        log::info!("reading asset from: {}", path.display());
        let bytes = std::fs::read(&path)?;
        Ok(bytes)
    }

    #[inline]
    pub fn extension(&self) -> Option<&str> {
        self.asset_path.path.extension()
    }
}

pub struct AssetCursorChildren<'a, 'b> {
    cursor: &'b mut AssetCursor<'a>,
    children: Vec<AssetPath>,
}

impl<'a, 'b> AssetCursorChildren<'a, 'b> {
    #[inline]
    pub fn next<'c>(&'c mut self) -> Option<AssetCursorChild<'a, 'b, 'c>> {
        self.children.pop().map(move |ap| AssetCursorChild {
            children: self,
            asset_path: ap,
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
    // TODO: allow multiple assets? -> include mut ref to sync queue assets (for table / collections)?
}

impl<'a> AssetCursor<'a> {
    #[inline]
    pub fn asset_path(&self) -> &AssetPath {
        &self.asset_path
    }

    #[inline]
    pub fn assets(&self) -> &Assets {
        self.assets
    }

    #[inline]
    pub fn assets_mut(&mut self) -> &mut Assets {
        self.assets
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
}

pub trait AssetLoader: Sized + Send + Sync + 'static {
    type Asset: Sized + Send + Sync + 'static;

    fn load<'a>(&self, cursor: &mut AssetCursor<'a>) -> anyhow::Result<Self::Asset>;
}

// Required to provide serde deserializer context for recursive asset loading
thread_local!(static SERDE_THREAD_LOCAL: RefCell<Option<SerdeThreadLocal>> = RefCell::new(None));

pub struct SerdeThreadLocal {
    assets: Assets,
}

pub struct SerdeAssetLoader<T> {
    _pd: PhantomData<T>,
}

impl<T: DeserializeOwned + Send + Sync + 'static> AssetLoader for SerdeAssetLoader<T> {
    type Asset = T;

    #[inline]
    fn load<'a>(&self, cursor: &mut AssetCursor<'a>) -> anyhow::Result<Self::Asset> {
        let bytes = cursor.read()?;

        let extension = cursor.extension().ok_or_else(|| {
            anyhow::anyhow!(
                "could not derive file type for serde asset loader: {}",
                cursor.asset_path
            )
        })?;

        SERDE_THREAD_LOCAL.with(|stl| {
            *stl.borrow_mut() = Some(SerdeThreadLocal {
                assets: cursor.assets.to_owned(),
            });

            let asset = match extension {
                "json" => serde_json::from_slice(&bytes)?,
                s => Err(anyhow::anyhow!(
                    "unhandled file type for serde asset loader: {}",
                    s
                ))?,
            };

            Ok(asset)
        })
    }
}

impl<T> Default for SerdeAssetLoader<T> {
    #[inline]
    fn default() -> Self {
        Self {
            _pd: Default::default(),
        }
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
                Some(tls) => Ok(tls.assets.client().upgrade(&weak)),
                None => Err(serde::de::Error::custom(
                    "strong asset ids can only be deserialized by the asset server",
                )),
            }
        })
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

    fn load<'a>(&self, cursor: &mut AssetCursor<'a>) -> anyhow::Result<Self::Asset> {
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
    fn load<'a>(&self, cursor: &mut AssetCursor<'a>) -> anyhow::Result<Self::Asset> {
        let weak_loader: AssetTableLoader<T, Weak> = AssetTableLoader::default();
        let weak_table = weak_loader.load(cursor)?;

        let assets = cursor.assets.client();
        let strong_table = weak_table
            .0
            .into_iter()
            .map(|(key, weak)| (key, assets.upgrade(&weak)))
            .collect();

        Ok(AssetTable(strong_table))
    }
}

impl<T, S> Default for AssetTableLoader<T, S> {
    fn default() -> Self {
        Self {
            _pd_t: Default::default(),
            _pd_s: Default::default(),
        }
    }
}
