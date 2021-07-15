pub mod loader;
pub mod notify;
pub mod storage;

use crate::asset::loader::{AssetLoader, AssetTableLoader, SerdeAssetLoader};
use crate::asset::notify::AssetChangeNotify;
use crate::asset::storage::{Assets, InnerAssets};
use crate::platform::action::ActionsConfig;
use crate::platform::DisplayConfig;
use crate::prelude::Font;
use crate::render::mesh::MeshLoader;
use crate::render::pipeline::{Pipeline, WGSLSourceLoader};
use crate::render::view::{ImageLoader, Texture};
use crate::time::TimeServer;
use crate::util::HashMap;
use crate::InitEvent;
use crate::{ok_or_continue, some_or_break};
use internment::Intern;
use parking_lot::Mutex;
use relative_path::{RelativePath, RelativePathBuf};
use roundabout::prelude::*;
use serde::de::DeserializeOwned;
use std::any::{Any, TypeId};
use std::cmp::Ordering;
use std::convert::TryInto;
use std::fmt::{Debug, Formatter};
use std::hash::Hasher;
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

// TODO: mutable vs immutable assets (user)

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AssetIdKind {
    Path(Intern<RelativePathBuf>),
    Uuid(Uuid),
}

impl AssetIdKind {
    #[inline]
    pub fn path(&self) -> Option<Intern<RelativePathBuf>> {
        if let AssetIdKind::Path(path) = self {
            Some(*path)
        } else {
            None
        }
    }

    #[inline]
    pub fn uuid(&self) -> Option<Uuid> {
        if let AssetIdKind::Uuid(uuid) = self {
            Some(*uuid)
        } else {
            None
        }
    }
}

impl Debug for AssetIdKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetIdKind::Path(path) => f.debug_tuple("Path").field(path.deref()).finish(),
            AssetIdKind::Uuid(uuid) => f.debug_tuple("Uuid").field(uuid).finish(),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct UntypedAssetId {
    // Optimization: use smaller hash that still guarantees no benign collisions
    id: [u8; blake3::OUT_LEN],
    kind: AssetIdKind,
    tid: TypeId,
}

impl UntypedAssetId {
    fn new(tid: TypeId, kind: AssetIdKind) -> Self {
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
        kind.hash(&mut hasher);
        let hash = hasher.0.finalize();

        UntypedAssetId {
            id: hash.into(),
            tid,
            kind,
        }
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
        let id1 = u128::from_ne_bytes(self.id[..16].try_into().unwrap());
        let id2 = u128::from_ne_bytes(self.id[16..].try_into().unwrap());

        f.debug_struct("UntypedAssetId")
            .field("id", &format!("{:032x}{:032x}", id1, id2))
            .field("kind", &self.kind)
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

// TODO: add LoadedAssetId that guarantees availability?
pub type WeakAssetId<T> = AssetId<T, Weak>;
pub type StrongAssetId<T> = AssetId<T, Strong>;

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
    pub fn kind(&self) -> AssetIdKind {
        self.untyped.kind
    }
}

impl<T: 'static> AssetId<T, Weak> {
    fn new(kind: AssetIdKind) -> Self {
        let untyped = UntypedAssetId::new(TypeId::of::<T>(), kind);
        Self {
            untyped,
            strength: Weak,
            _pd: Default::default(),
        }
    }

    #[inline]
    pub fn new_path<P: Into<RelativePathBuf>>(path: P) -> Self {
        Self::new(AssetIdKind::Path(Intern::new(path.into())))
    }

    #[inline]
    pub fn new_uuid(uuid: Uuid) -> Self {
        Self::new(AssetIdKind::Uuid(uuid))
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
        AssetId::new(AssetIdKind::Uuid(uuid))
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
            .on(on_timed_gc_assets_event)
            .on(on_timed_notify_assets_event)
            .init_fn(move |context| AssetServer {
                asset_dir: Default::default(),
                loaders,
                assets: Assets {
                    inner: Arc::new(Default::default()),
                    sender: context.sender().clone(),
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
                    .kind
                    .path()
                    .ok_or_else(|| anyhow::anyhow!("asset path to load not found"))?;
                let asset = loader.load(ap, &rp, a)?;
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
    dyn FnMut(&Path, UntypedAssetId, &Assets, &mut RuntimeContext) -> anyhow::Result<SyncQueueEntry>
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
    asset_dir: PathBuf,
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
    state.asset_dir = event.asset_dir.clone();
    log::info!("assets created");
    context.sender().send(AssetsCreatedEvent {
        inner: state.assets.inner.clone(),
    });

    TimeServer::schedule(state.gc_schedule, GcAssetsEvent, context.sender());

    if let Some(notify) = &mut state.notify {
        log::info!("start watching assets for changes");
        notify
            .watch(&state.asset_dir)
            .expect("watchable asset dir for hot reloading");
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

            let sync_queue_entry =
                match (loader)(&state.asset_dir, event.id.untyped, &state.assets, context) {
                    Ok(ok) => ok,
                    Err(e) => {
                        log::error!(
                            "Could not load asset {:?}: {}",
                            event.id.untyped.kind.path(),
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

fn on_timed_gc_assets_event(
    state: &mut AssetServer,
    context: &mut RuntimeContext,
    _event: &GcAssetsEvent,
) {
    log::debug!("start assets gc");
    state.gc_at = state.assets.gc(state.gc_at, state.gc_max);
    TimeServer::schedule(state.gc_schedule, GcAssetsEvent, context.sender());
}

fn on_timed_notify_assets_event(
    state: &mut AssetServer,
    context: &mut RuntimeContext,
    _event: &NotifyAssetsEvent,
) {
    log::debug!("start checking asset changes");
    let asset_dir = &state.asset_dir;
    let mut changed_assets: Vec<UntypedAssetId> = Vec::default();
    for changed in state.notify.as_mut().unwrap().changes_iter() {
        let mut relative_path_string = ok_or_continue!(changed.path.strip_prefix(asset_dir));
        loop {
            if let Some(relative_path_string) = relative_path_string.to_str() {
                let path = Intern::new(RelativePath::new(relative_path_string).to_owned());
                changed_assets.extend(state.assets.asset_ids_for_path(path).drain(..));
            }
            // we check if any parent folder is an asset
            relative_path_string = some_or_break!(relative_path_string.parent());
        }
    }

    for asset_id in changed_assets {
        let loader = state
            .loaders
            .get_mut(&asset_id.tid)
            .expect("asset loader for a reload");

        let sync_queue_entry = match (loader)(asset_dir, asset_id, &state.assets, context) {
            Ok(ok) => ok,
            Err(e) => {
                log::error!("Could not load asset {:?}: {}", asset_id.kind.path(), e);
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
}

impl AssetsCreatedEvent {
    #[inline]
    pub fn assets(&self, sender: MessageSender) -> Assets {
        Assets {
            inner: self.inner.clone(),
            sender,
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
