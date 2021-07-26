pub mod loader;
pub mod notify;
pub mod storage;

use crate::asset::loader::{AssetLoader, AssetTableLoader, SerdeAssetLoader};
use crate::asset::notify::AssetChangeNotify;
use crate::asset::storage::{Assets, AssetsPaths, InnerAssets};
use crate::platform::action::ActionsConfig;
use crate::platform::DisplayConfig;
use crate::prelude::Font;
use crate::render::mesh::MeshLoader;
use crate::render::pipeline::{Pipeline, WGSLSourceLoader};
use crate::render::view::{ImageLoader, Texture};
use crate::time::TimeServer;
use crate::util::HashMap;
use crate::InitEvent;
use crate::{some_or_break, some_or_continue};
use internment::Intern;
use parking_lot::Mutex;
use relative_path::{RelativePath, RelativePathBuf};
use roundabout::prelude::*;
use serde::de::{DeserializeOwned, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::any::{Any, TypeId};
use std::cmp::Ordering;
use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter};
use std::hash::Hasher;
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

// TODO: mutable vs immutable assets (user)
//  6. add save for UserAssets // errors? via events -> add load errors as well? -> typed?

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AssetPathKind {
    Sys,
    Usr,
}

impl AssetPathKind {
    #[inline]
    pub fn protocol(&self) -> &'static str {
        match self {
            AssetPathKind::Sys => "sys",
            AssetPathKind::Usr => "usr",
        }
    }

    #[inline]
    pub fn from_protocol(protocol: &str) -> Option<Self> {
        match protocol {
            "sys" => Some(Self::Sys),
            "usr" => Some(Self::Usr),
            _ => None,
        }
    }
}

pub trait AssetPathParam {
    fn path(self) -> Intern<RelativePathBuf>;
}

impl AssetPathParam for RelativePathBuf {
    #[inline]
    fn path(self) -> Intern<RelativePathBuf> {
        Intern::new(self)
    }
}

impl AssetPathParam for &RelativePathBuf {
    #[inline]
    fn path(self) -> Intern<RelativePathBuf> {
        Intern::new(self.to_owned())
    }
}

impl AssetPathParam for &RelativePath {
    #[inline]
    fn path(self) -> Intern<RelativePathBuf> {
        Intern::new(self.to_owned())
    }
}

impl AssetPathParam for String {
    #[inline]
    fn path(self) -> Intern<RelativePathBuf> {
        Intern::new(self.into())
    }
}

impl AssetPathParam for &String {
    #[inline]
    fn path(self) -> Intern<RelativePathBuf> {
        Intern::new(self.into())
    }
}

impl AssetPathParam for &str {
    #[inline]
    fn path(self) -> Intern<RelativePathBuf> {
        Intern::new(self.into())
    }
}

impl AssetPathParam for Intern<RelativePathBuf> {
    #[inline]
    fn path(self) -> Intern<RelativePathBuf> {
        self
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AssetPath {
    kind: AssetPathKind,
    path: Intern<RelativePathBuf>,
}

impl AssetPath {
    #[inline]
    pub fn new(kind: AssetPathKind, path: Intern<RelativePathBuf>) -> Self {
        Self { kind, path }
    }

    #[inline]
    pub fn kind(&self) -> AssetPathKind {
        self.kind
    }

    #[inline]
    pub fn path(&self) -> Intern<RelativePathBuf> {
        self.path
    }

    #[inline]
    pub fn from_uri<T: AsRef<str>>(uri: T) -> anyhow::Result<Self> {
        let uri = uri.as_ref();
        let (protocol, path) = uri
            .split_once("://")
            .ok_or_else(|| anyhow::anyhow!("malformed asset path uri {}", uri))?;
        let kind = AssetPathKind::from_protocol(protocol).ok_or_else(|| {
            anyhow::anyhow!("unknown asset protocol of {} for path {}", protocol, path)
        })?;

        Ok(Self {
            kind,
            path: path.path(),
        })
    }

    #[inline]
    pub fn sys<T: AssetPathParam>(path: T) -> Self {
        Self {
            kind: AssetPathKind::Sys,
            path: path.path(),
        }
    }

    #[inline]
    pub fn usr<T: AssetPathParam>(path: T) -> Self {
        Self {
            kind: AssetPathKind::Usr,
            path: path.path(),
        }
    }
}

impl FromStr for AssetPath {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_uri(s)
    }
}

impl TryFrom<&str> for AssetPath {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_uri(s)
    }
}

impl Debug for AssetPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for AssetPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.kind.protocol())?;
        f.write_str("://")?;
        f.write_str(self.path.deref().as_str())
    }
}

struct AssetPathVisitor;

impl<'de> Visitor<'de> for AssetPathVisitor {
    type Value = AssetPath;

    #[inline]
    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("asset path uri string")
    }

    #[inline]
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        AssetPath::from_uri(v).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for AssetPath {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<AssetPath, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(AssetPathVisitor)
    }
}

impl Serialize for AssetPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AssetUri {
    AssetPath(AssetPath),
    Uuid(Uuid),
}

impl AssetUri {
    #[inline]
    pub fn from_uri<T: AsRef<str>>(uri: T) -> anyhow::Result<Self> {
        let uri = uri.as_ref();
        if let Some(uuid) = uri.strip_prefix("uuid://") {
            let uuid = Uuid::parse_str(uuid)?;
            Ok(Self::Uuid(uuid))
        } else {
            let asset_path = AssetPath::from_uri(uri)?;
            Ok(Self::AssetPath(asset_path))
        }
    }

    #[inline]
    pub fn asset_path(&self) -> Option<AssetPath> {
        if let AssetUri::AssetPath(path) = self {
            Some(*path)
        } else {
            None
        }
    }

    #[inline]
    pub fn uuid(&self) -> Option<Uuid> {
        if let AssetUri::Uuid(uuid) = self {
            Some(*uuid)
        } else {
            None
        }
    }
}

impl FromStr for AssetUri {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_uri(s)
    }
}

impl TryFrom<&str> for AssetUri {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_uri(s)
    }
}

impl Debug for AssetUri {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for AssetUri {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetUri::AssetPath(path) => Display::fmt(path, f),
            AssetUri::Uuid(uuid) => {
                f.write_str("uuid://")?;
                Display::fmt(uuid, f)
            }
        }
    }
}

struct AssetUriVisitor;

impl<'de> Visitor<'de> for AssetUriVisitor {
    type Value = AssetUri;

    #[inline]
    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("asset uri string")
    }

    #[inline]
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        AssetUri::from_uri(v).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for AssetUri {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<AssetUri, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(AssetUriVisitor)
    }
}

impl Serialize for AssetUri {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct UntypedAssetId {
    // Optimization: use smaller hash that still guarantees no benign collisions
    id: [u8; 16],
    uri: AssetUri,
    tid: TypeId,
}

impl UntypedAssetId {
    fn new(tid: TypeId, uri: AssetUri) -> Self {
        // required as we otherwise can't hash tid
        struct Blake3StdHasher(blake3::Hasher);
        impl std::hash::Hasher for Blake3StdHasher {
            fn finish(&self) -> u64 {
                unreachable!();
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.update(bytes);
            }
        }

        use std::hash::Hash;
        let mut hasher = Blake3StdHasher(blake3::Hasher::default());
        tid.hash(&mut hasher);
        uri.hash(&mut hasher);
        let mut id = [0; 16];
        hasher.0.finalize_xof().fill(&mut id);

        UntypedAssetId { id, tid, uri }
    }
}

impl std::hash::Hash for UntypedAssetId {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Debug for UntypedAssetId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let id = u128::from_ne_bytes(self.id);

        f.debug_struct("UntypedAssetId")
            .field("id", &format!("{:032x}", id))
            .field("kind", &self.uri)
            .field("tid", &self.tid)
            .finish()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Weak;

#[derive(Clone)]
pub struct Strong(Arc<()>);

impl Debug for Strong {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Strong")
    }
}

#[derive(Clone)]
pub struct Loaded(Arc<()>);

impl Debug for Loaded {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Loaded")
    }
}

pub type WeakAssetId<T> = AssetId<T, Weak>;
pub type StrongAssetId<T> = AssetId<T, Strong>;
pub type LoadedAssetId<T> = AssetId<T, Loaded>;

pub struct AssetId<T, S> {
    untyped: UntypedAssetId,
    strength: S,
    _pd: PhantomData<T>,
}

impl<T, S> AssetId<T, S> {
    #[inline]
    pub fn is_same_asset<S2>(&self, other: &AssetId<T, S2>) -> bool {
        self.untyped == other.untyped
    }

    #[inline]
    pub fn uri(&self) -> AssetUri {
        self.untyped.uri
    }
}

impl<T: 'static> AssetId<T, Weak> {
    fn new(uri: AssetUri) -> Self {
        let untyped = UntypedAssetId::new(TypeId::of::<T>(), uri);
        Self {
            untyped,
            strength: Weak,
            _pd: Default::default(),
        }
    }

    #[inline]
    pub fn path(path: AssetPath) -> Self {
        Self::new(AssetUri::AssetPath(path))
    }

    #[inline]
    pub fn uuid(uuid: Uuid) -> Self {
        Self::new(AssetUri::Uuid(uuid))
    }

    unsafe fn from_untyped(untyped: UntypedAssetId) -> Self {
        Self {
            untyped,
            strength: Weak,
            _pd: Default::default(),
        }
    }

    fn into_strong(self, counter: Arc<()>) -> StrongAssetId<T> {
        AssetId {
            untyped: self.untyped,
            strength: Strong(counter),
            _pd: Default::default(),
        }
    }
}

impl<T: 'static> AssetId<T, Loaded> {
    #[inline]
    pub fn as_strong(&self) -> StrongAssetId<T> {
        AssetId {
            untyped: self.untyped,
            strength: Strong(self.strength.0.clone()),
            _pd: Default::default(),
        }
    }
}

impl<T: 'static> AssetId<T, Strong> {
    unsafe fn into_loaded(self) -> LoadedAssetId<T> {
        AssetId {
            untyped: self.untyped,
            strength: Loaded(self.strength.0),
            _pd: Default::default(),
        }
    }
}

impl<T: 'static, S> AssetId<T, S> {
    #[inline]
    pub fn to_weak(&self) -> WeakAssetId<T> {
        AssetId {
            untyped: self.untyped,
            strength: Weak,
            _pd: Default::default(),
        }
    }
}

impl<T: 'static> From<Uuid> for AssetId<T, Weak> {
    #[inline]
    fn from(uuid: Uuid) -> Self {
        AssetId::uuid(uuid)
    }
}

impl<T, S: Debug> Debug for AssetId<T, S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetId")
            .field("strength", &self.strength)
            .field("untyped", &self.untyped)
            .finish()
    }
}

impl<T, S> Eq for AssetId<T, S> {}

impl<T, S> PartialEq for AssetId<T, S> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // untyped includes a hash that is based on the origin and type
        self.untyped.eq(&other.untyped)
    }
}

impl<T, S> Ord for AssetId<T, S> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        // untyped includes a hash that is based on the origin and type
        self.untyped.cmp(&other.untyped)
    }
}

impl<T, S> PartialOrd for AssetId<T, S> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T, S> std::hash::Hash for AssetId<T, S> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.untyped.hash(state);
    }
}

impl<T, S: Clone> Clone for AssetId<T, S> {
    fn clone(&self) -> Self {
        Self {
            untyped: self.untyped,
            strength: self.strength.clone(),
            _pd: Default::default(),
        }
    }
}

impl<T, S: Copy> Copy for AssetId<T, S> {}

pub struct AssetServerBuilder {
    handler: OpenMessageHandlerBuilder<AssetServer>,
    loaders: HashMap<TypeId, UntypedLoader>,
    sync_queue_max: usize,
    gc_schedule: Duration,
    gc_max: usize,
    hot_reloading: bool,
}

impl AssetServerBuilder {
    pub fn with_sync_queue_max(mut self, sync_queue_max: usize) -> Self {
        self.sync_queue_max = sync_queue_max;
        self
    }

    pub fn with_gc_schedule(mut self, gc_schedule: Duration) -> Self {
        self.gc_schedule = gc_schedule;
        self
    }

    pub fn with_gc_max(mut self, gc_max: usize) -> Self {
        self.gc_max = gc_max;
        self
    }

    pub fn with_hot_reloading(mut self, hot_reloading: bool) -> Self {
        self.hot_reloading = hot_reloading;
        self
    }

    pub fn add_serde<T: DeserializeOwned + Send + Sync + 'static>(self) -> Self {
        let asset_loader: SerdeAssetLoader<T> = SerdeAssetLoader::default();
        self.add(asset_loader)
    }

    pub fn add_strong_table<T: Send + Sync + 'static>(self) -> Self {
        let asset_loader: AssetTableLoader<T, Strong> = AssetTableLoader::default();
        self.add(asset_loader)
    }

    pub fn add_weak_table<T: Send + Sync + 'static>(self) -> Self {
        let asset_loader: AssetTableLoader<T, Weak> = AssetTableLoader::default();
        self.add(asset_loader)
    }

    pub fn add<T: AssetLoader>(mut self, asset_loader: T) -> Self {
        unsafe {
            self.insert_loader(asset_loader);
            self.handler = self.handler.on(on_load_asset_event::<T::Asset>);
        }
        self
    }

    pub fn finish(self) -> InitMessageHandlerBuilder<AssetServer> {
        let Self {
            handler,
            loaders,
            sync_queue_max,
            gc_schedule,
            gc_max,
            hot_reloading,
        } = self;

        let notify = if hot_reloading {
            Some(AssetChangeNotify::new().expect("hot asset loading"))
        } else {
            None
        };

        handler
            .on(on_init_event)
            .on(on_store_asset_event)
            .on(on_sync_asset_event)
            .on(on_gc_assets_event)
            .on(on_notify_assets_event)
            .init_fn(move |context| AssetServer {
                loaders,
                assets: Assets {
                    inner: Arc::new(Default::default()),
                    sender: context.sender().clone(),
                    paths: Rc::new(AssetsPaths {
                        // TODO: why don't we init the asset server sys / usr dirs with the builder?
                        sys_dir: Default::default(),
                        usr_dir: Default::default(),
                    }),
                },
                sync: 0,
                sync_requested: 0,
                sync_queue_asset: Default::default(),
                sync_queue_max,
                gc_at: 0,
                gc_schedule,
                gc_max,
                notify,
            })
    }

    unsafe fn insert_loader<T: AssetLoader>(&mut self, loader: T) {
        self.loaders.insert(
            TypeId::of::<T::Asset>(),
            Box::new(move |ap, id, a, es| {
                let rp = id
                    .uri
                    .asset_path()
                    .ok_or_else(|| anyhow::anyhow!("asset path to load not found"))?;
                let asset = loader.load(ap, rp.path.deref(), a)?;
                let typed_id: WeakAssetId<T::Asset> = WeakAssetId::from_untyped(id);

                let loaded_event = es.sender().prepare(AssetEvent {
                    id: typed_id,
                    kind: AssetEventKind::Load,
                });

                let unloaded_event = es.sender().prepare(AssetEvent {
                    id: typed_id,
                    kind: AssetEventKind::Unload,
                });

                Ok(SyncQueueEntry {
                    asset_id: id,
                    asset: Box::new(asset),
                    loaded_event,
                    unloaded_event,
                })
            }),
        );
    }
}

type UntypedLoader = Box<
    dyn FnMut(
            &Path,
            UntypedAssetId,
            &mut Assets,
            &mut RuntimeContext,
        ) -> anyhow::Result<SyncQueueEntry>
        + Send,
>;

type UntypedAsset = Box<dyn std::any::Any + 'static + Send + Sync>;

pub(crate) struct SyncQueueEntry {
    asset_id: UntypedAssetId,
    asset: UntypedAsset,
    loaded_event: Option<UntypedMessage>,
    unloaded_event: Option<UntypedMessage>,
}

pub struct AssetServer {
    loaders: HashMap<TypeId, UntypedLoader>,
    assets: Assets,
    sync: u64,
    sync_requested: u64,
    sync_queue_asset: Vec<SyncQueueEntry>,
    sync_queue_max: usize,
    gc_at: usize,
    gc_schedule: Duration,
    gc_max: usize,
    notify: Option<AssetChangeNotify>,
}

impl AssetServer {
    pub fn empty(handler: OpenMessageHandlerBuilder<AssetServer>) -> AssetServerBuilder {
        AssetServerBuilder {
            handler,
            loaders: Default::default(),
            sync_queue_max: usize::MAX,
            gc_schedule: Duration::from_secs(1),
            gc_max: usize::MAX,
            hot_reloading: true,
        }
    }

    pub fn builder(handler: OpenMessageHandlerBuilder<AssetServer>) -> AssetServerBuilder {
        Self::empty(handler)
            .add_serde::<DisplayConfig>()
            .add_serde::<ActionsConfig>()
            .add_serde::<Texture>()
            .add_serde::<Pipeline>()
            .add_serde::<Font>()
            .add(WGSLSourceLoader)
            .add(MeshLoader)
            .add(ImageLoader)
    }
}

fn on_init_event(state: &mut AssetServer, context: &mut RuntimeContext, event: &InitEvent) {
    state.assets.paths = Rc::new(AssetsPaths {
        sys_dir: event.sys_dir.clone(),
        usr_dir: event.usr_dir.clone(),
    });

    log::info!("assets created");
    context.sender().send(AssetsCreatedEvent {
        inner: state.assets.inner.clone(),
        sys_dir: state.assets.paths.sys_dir.clone(),
        usr_dir: state.assets.paths.usr_dir.clone(),
    });

    TimeServer::schedule(state.gc_schedule, GcAssetsEvent, context.sender());

    if let Some(notify) = &mut state.notify {
        log::info!("start watching assets for changes");
        if let Err(e) = notify.watch(&state.assets.paths.sys_dir) {
            log::warn!(
                "could not watch sys asset dir {}: {}",
                state.assets.paths.sys_dir.display(),
                e
            )
        }

        if let Err(e) = notify.watch(&state.assets.paths.usr_dir) {
            log::warn!(
                "could not watch usr asset dir {}: {}",
                state.assets.paths.sys_dir.display(),
                e
            )
        }

        TimeServer::schedule(
            Duration::from_millis(500),
            NotifyAssetsEvent,
            context.sender(),
        );
    }
}

fn on_load_asset_event<T: 'static + Send + Sync>(
    state: &mut AssetServer,
    context: &mut RuntimeContext,
    event: &LoadAssetEvent<T>,
) {
    match state.loaders.get_mut(&TypeId::of::<T>()) {
        Some(loader) => {
            if !event.force && state.assets.client().has(&event.id) {
                return;
            }

            let paths = state.assets.paths.clone();
            let asset_dir = if let Some(asset_path) = event.id.uri().asset_path() {
                paths.asset_dir(&asset_path.kind)
            } else {
                log::error!(
                    "Can't load asset without an asset path {}",
                    event.id.untyped.uri
                );
                return;
            };

            let sync_queue_entry =
                match (loader)(asset_dir, event.id.untyped, &mut state.assets, context) {
                    Ok(ok) => ok,
                    Err(e) => {
                        // TODO: panic? -> this is probably required for LoadedAssetTables otherwise how to fail the loading?
                        //  -> dependency registration?
                        log::error!("Could not load asset {}: {}", event.id.untyped.uri, e);
                        return;
                    }
                };

            state.sync_queue_asset.push(sync_queue_entry);
            state.sync_requested += 1;
            context.sender().send(SyncAssetEvent {
                sync: state.sync_requested,
            });
        }
        None => {
            log::error!("AssetLoader not found for: {}", std::any::type_name::<T>());
        }
    }
}

fn on_store_asset_event(
    state: &mut AssetServer,
    context: &mut RuntimeContext,
    event: &StoreAssetEvent,
) {
    let sync_queue_entry = SyncQueueEntry {
        asset_id: event.id,
        asset: event.asset.lock().take().expect("store asset"),
        loaded_event: event.load_event.lock().take(),
        unloaded_event: event.unload_event.lock().take(),
    };
    state.sync_queue_asset.push(sync_queue_entry);
    state.sync_requested += 1;
    context.sender().send(SyncAssetEvent {
        sync: state.sync_requested,
    });
}

fn on_sync_asset_event(
    state: &mut AssetServer,
    _context: &mut RuntimeContext,
    event: &SyncAssetEvent,
) {
    if state.sync_requested != event.sync && state.sync_queue_asset.len() < state.sync_queue_max {
        log::debug!(
            "skip intermediate asset sync of {} as {} is already requested",
            event.sync,
            state.sync_requested
        );
        return;
    }

    if state.sync_queue_asset.is_empty() {
        return;
    }

    log::debug!(
        "sync assets from {} to {}",
        state.sync,
        state.sync_requested
    );
    state.sync = state.sync_requested;

    unsafe { state.assets.extend(state.sync_queue_asset.drain(..)) };
}

fn on_gc_assets_event(
    state: &mut AssetServer,
    context: &mut RuntimeContext,
    _event: &GcAssetsEvent,
) {
    log::debug!("start assets gc");
    state.gc_at = state.assets.gc(state.gc_at, state.gc_max);
    TimeServer::schedule(state.gc_schedule, GcAssetsEvent, context.sender());
}

fn on_notify_assets_event(
    state: &mut AssetServer,
    context: &mut RuntimeContext,
    _event: &NotifyAssetsEvent,
) {
    log::debug!("start checking asset changes");

    let mut changed_assets: Vec<UntypedAssetId> = Vec::default();
    for changed in state.notify.as_mut().unwrap().changes_iter() {
        let assets = &state.assets;
        let mut asset_path = some_or_continue!(assets.paths.asset_path(&changed.path));
        let asset_dir = assets.paths.asset_dir(&asset_path.kind);
        loop {
            changed_assets.extend(assets.asset_ids_for_path(asset_path).drain(..));
            // we check if any parent folder is an asset
            asset_path = some_or_break!(asset_path
                .path
                .parent()
                .and_then(|p| assets.paths.asset_path(&p.to_path(asset_dir))));
        }
    }

    for asset_id in changed_assets {
        let paths = state.assets.paths.clone();
        let asset_dir = paths.asset_dir(&asset_id.uri.asset_path().unwrap().kind);

        let loader = state
            .loaders
            .get_mut(&asset_id.tid)
            .expect("asset loader for a reload");

        let sync_queue_entry = match (loader)(asset_dir, asset_id, &mut state.assets, context) {
            Ok(ok) => ok,
            Err(e) => {
                log::error!(
                    "Could not load asset {:?}: {}",
                    asset_id.uri.asset_path(),
                    e
                );
                return;
            }
        };

        state.sync_queue_asset.push(sync_queue_entry);
        state.sync_requested += 1;
        context.sender().send(SyncAssetEvent {
            sync: state.sync_requested,
        });
    }

    TimeServer::schedule(
        Duration::from_millis(500),
        NotifyAssetsEvent,
        context.sender(),
    );
}

pub struct AssetsCreatedEvent {
    inner: Arc<InnerAssets>,
    sys_dir: PathBuf,
    usr_dir: PathBuf,
}

impl AssetsCreatedEvent {
    /// usage of multiple assets in the same thread can result in deadlocks
    #[inline]
    pub fn assets(&self, sender: MessageSender) -> Assets {
        Assets {
            inner: self.inner.clone(),
            sender,
            paths: Rc::new(AssetsPaths {
                sys_dir: self.sys_dir.clone(),
                usr_dir: self.usr_dir.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadAssetEvent<T> {
    pub id: AssetId<T, Weak>,
    pub force: bool,
}

impl<T> LoadAssetEvent<T> {
    fn new(id: AssetId<T, Weak>, force: bool) -> Self {
        Self { id, force }
    }
}

pub struct StoreAssetEvent {
    id: UntypedAssetId,
    asset: Mutex<Option<Box<dyn Any + Send + Sync>>>,
    load_event: Mutex<Option<UntypedMessage>>,
    unload_event: Mutex<Option<UntypedMessage>>,
}

impl StoreAssetEvent {
    fn new<T: Send + Sync + 'static>(
        id: AssetId<T, Weak>,
        asset: T,
        sender: &MessageSender,
    ) -> Self {
        let load_event = sender.prepare(AssetEvent {
            id,
            kind: AssetEventKind::Load,
        });

        let unload_event = sender.prepare(AssetEvent {
            id,
            kind: AssetEventKind::Unload,
        });

        Self {
            id: id.untyped,
            asset: Mutex::new(Some(Box::new(asset))),
            load_event: Mutex::new(load_event),
            unload_event: Mutex::new(unload_event),
        }
    }
}

struct SyncAssetEvent {
    pub sync: u64,
}

struct GcAssetsEvent;

struct NotifyAssetsEvent;

#[derive(Debug, Clone)]
pub struct AssetEvent<T> {
    pub id: AssetId<T, Weak>,
    pub kind: AssetEventKind,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AssetEventKind {
    Load,
    Unload,
}
