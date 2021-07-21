use crate::asset::storage::{Assets, AssetsClient};
use crate::asset::{
    AssetId, AssetPath, AssetPathVisitor, AssetUri, Loaded, Strong, StrongAssetId, Weak,
    WeakAssetId,
};
use crate::util::IndexMap;
use internment::Intern;
use relative_path::RelativePath;
use relative_path::RelativePathBuf;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::Path;

pub trait AssetLoader: Sized + Send + Sync + 'static {
    type Asset: Sized + Send + Sync + 'static;

    #[inline]
    fn load<'a>(
        &self,
        asset_dir: &'a Path,
        rel_path: &'a RelativePath,
        assets: &'a Assets,
    ) -> anyhow::Result<Self::Asset> {
        // TODO: give struct that can read assets and the asset dirs instead of separation into two methods
        //  required for inline assets?
        let path = rel_path.to_path(asset_dir);
        log::info!(
            "loading asset of {} from: {}",
            std::any::type_name::<Self::Asset>(),
            path.display()
        );

        let bytes = std::fs::read(&path)?;
        self.deserialize(rel_path, bytes, assets)
    }

    fn deserialize<'a>(
        &self,
        path: &'a RelativePath,
        bytes: Vec<u8>,
        assets: &'a Assets,
    ) -> anyhow::Result<Self::Asset>;
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
    fn deserialize<'a>(
        &self,
        path: &'a RelativePath,
        bytes: Vec<u8>,
        assets: &'a Assets,
    ) -> anyhow::Result<Self::Asset> {
        let extension = path.extension().ok_or_else(|| {
            anyhow::anyhow!(
                "could not derive file type for serde asset loader: {}",
                path
            )
        })?;

        SERDE_THREAD_LOCAL.with(|stl| {
            *stl.borrow_mut() = Some(SerdeThreadLocal {
                assets: assets.to_owned(),
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
        let path = deserializer.deserialize_string(AssetPathVisitor)?;
        Ok(WeakAssetId::new(AssetUri::AssetPath(path)))
    }
}

impl<'de, T: Send + Sync + 'static> Deserialize<'de> for AssetId<T, Strong> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<StrongAssetId<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // TODO: also allow inlined assets  -> name + data for T => insert into Assets (check deadlocks)
        //  How? the api expects Vec<u8>

        let path = deserializer.deserialize_string(AssetPathVisitor)?;
        let weak: WeakAssetId<T> = WeakAssetId::new(AssetUri::AssetPath(path));

        SERDE_THREAD_LOCAL.with(|maybe_tls| {
            let borrow_maybe_tls = maybe_tls.borrow();
            match borrow_maybe_tls.deref() {
                Some(tls) => Ok(tls
                    .assets
                    .client()
                    .try_upgrade(&weak)
                    .expect("upgradable weak")),
                None => Err(serde::de::Error::custom(
                    "strong asset ids can only be deserialized by the asset server",
                )),
            }
        })
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
            // this should never fail as asset tables require path assets
            let strong = client.try_upgrade(v).unwrap();
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

    #[inline]
    fn load<'a>(
        &self,
        asset_dir: &'a Path,
        rel_path: &'a RelativePath,
        assets: &'a Assets,
    ) -> anyhow::Result<Self::Asset> {
        let path = rel_path.to_path(asset_dir);
        log::info!(
            "loading asset table of {} from: {}",
            std::any::type_name::<T>(),
            path.display()
        );

        let asset_path_kind = assets
            .asset_path_kind(asset_dir)
            .expect("asset dir to be known");

        let mut underlying = IndexMap::default();
        let dir = std::fs::read_dir(path)?;
        for entry in dir {
            let entry = entry?;
            let entry_path = entry.path();
            if !entry_path.is_file() {
                continue;
            }

            if let Some(file_name) = entry_path.file_name().and_then(|s| s.to_str()) {
                let entry_rel_path = Intern::new(rel_path.join(file_name));
                let entry_asset_id = WeakAssetId::new(AssetUri::AssetPath(AssetPath::new(
                    asset_path_kind,
                    entry_rel_path,
                )));
                underlying.insert(entry_rel_path, entry_asset_id);
            }
        }

        Ok(AssetTable(underlying))
    }

    fn deserialize<'a>(
        &self,
        _path: &'a RelativePath,
        _bytes: Vec<u8>,
        _assets: &'a Assets,
    ) -> anyhow::Result<Self::Asset> {
        unreachable!();
    }
}

impl<T: Send + Sync + 'static> AssetLoader for AssetTableLoader<T, Strong> {
    type Asset = AssetTable<T, Strong>;

    #[inline]
    fn load<'a>(
        &self,
        asset_dir: &'a Path,
        rel_path: &'a RelativePath,
        assets: &'a Assets,
    ) -> anyhow::Result<Self::Asset> {
        let weak_loader: AssetTableLoader<T, Weak> = AssetTableLoader::default();
        let weak_table = weak_loader.load(asset_dir, rel_path, assets)?;

        let assets = assets.client();
        let mut strong_table = IndexMap::default();
        for (key, weak) in weak_table.0 {
            let strong = assets.try_upgrade(&weak).expect("upgradable weak");
            strong_table.insert(key, strong);
        }

        Ok(AssetTable(strong_table))
    }

    fn deserialize<'a>(
        &self,
        _path: &'a RelativePath,
        _bytes: Vec<u8>,
        _assets: &'a Assets,
    ) -> anyhow::Result<Self::Asset> {
        unreachable!();
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
