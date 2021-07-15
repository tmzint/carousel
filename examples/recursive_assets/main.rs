use carousel::prelude::*;
use serde::Deserialize;

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .filter_module("wgpu_core", log::LevelFilter::Warn)
        .init();

    let runtime = Runtime::builder(2_097_152)
        .add(RenderServer::new)
        .add(TimeServer::new)
        .add(|b| {
            AssetServer::builder(b)
                .add_serde::<RecursiveAsset>()
                .add_serde::<ChildAsset>()
                .add_weak_table::<ChildAsset>()
                .finish()
        })
        .add_group(|mut g| {
            let resource = SimResource {
                setup_builder: g.register(SetupState::handler),
                loading_builder: g.register(LoadingState::handler),
            };
            SimServer::builder(g, || resource).init_fn(|resources| {
                let setup_builder = resources.resource.setup_builder.clone();
                State::from(setup_builder.init_finish(resources, SetupState).unwrap())
            })
        })
        .finish_main_group(|g| PlatformServer::new("display.json", ActionsConfig::default(), g));

    Engine::builder()?
        .with_asset_path("carousel/examples/recursive_assets/assets/")
        .with_runtime(runtime)
        .finish()
        .start()
}

pub struct SimResource {
    setup_builder: ClosedSimHandlerBuilder<SetupState, SimResource, State>,
    loading_builder: ClosedSimHandlerBuilder<LoadingState, SimResource, State>,
}

enum State {
    Setup(SimHandler<SetupState, SimResource, State>),
    Loading(SimHandler<LoadingState, SimResource, State>),
}

impl SimState<SimResource> for State {
    fn handle<M: MessageView>(
        &mut self,
        resources: &mut SimResources<SimResource>,
        message: &M,
    ) -> Option<StateInstruction<Self>> {
        match self {
            State::Setup(s) => s.handle(resources, message),
            State::Loading(s) => s.handle(resources, message),
        }
    }
}

impl From<SimHandler<SetupState, SimResource, State>> for State {
    fn from(s: SimHandler<SetupState, SimResource, State>) -> Self {
        Self::Setup(s)
    }
}

impl From<SimHandler<LoadingState, SimResource, State>> for State {
    fn from(s: SimHandler<LoadingState, SimResource, State>) -> Self {
        Self::Loading(s)
    }
}

struct SetupState;

impl SetupState {
    fn handler(
        h: OpenSimHandlerBuilder<SetupState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<SetupState, SimResource, State> {
        h.on(Self::on_frame_requested_event)
    }

    fn on_frame_requested_event(
        _state: &mut SetupState,
        resources: &mut SimResources<SimResource>,
        _event: &FrameRequestedEvent,
    ) -> StateInstruction<State> {
        let assets = resources.assets.client();

        let asset = assets.load("recursive.json");
        let table = assets.load("recursive");

        let loading = LoadingState { asset, table };
        let loading_builder = resources.resource.loading_builder.clone();
        StateInstruction::pop_push(loading_builder.init_finish(resources, loading).unwrap())
    }
}

pub struct LoadingState {
    asset: StrongAssetId<RecursiveAsset>,
    table: StrongAssetId<WeakAssetTable<ChildAsset>>,
}

impl LoadingState {
    fn handler(
        h: OpenSimHandlerBuilder<LoadingState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<LoadingState, SimResource, State> {
        h.on(Self::on_frame_requested_event)
    }

    fn on_frame_requested_event(
        state: &mut LoadingState,
        resources: &mut SimResources<SimResource>,
        _event: &FrameRequestedEvent,
    ) -> StateInstruction<State> {
        let assets = resources.assets.client();

        if let Some(recursive) = assets.get(&state.asset) {
            for (i, eager) in recursive.eager.iter().enumerate() {
                println!("eager[{}]: {:?}", i, assets.get(eager));
            }
            for (i, lazy) in recursive.lazy.iter().enumerate() {
                let lazy_loading = assets.try_upgrade(lazy);
                println!("lazy[{}]: {:?}", i, assets.get(&lazy_loading.unwrap()));
            }
        }

        if let Some(table) = assets.get(&state.table) {
            for (k, v) in table.iter() {
                println!("table[{}]: {:?}", k, v);
            }
            resources.context.shutdown_switch().request_shutdown();
        }

        StateInstruction::Stay
    }
}

#[derive(Deserialize)]
pub struct RecursiveAsset {
    eager: Vec<StrongAssetId<ChildAsset>>,
    lazy: Vec<WeakAssetId<ChildAsset>>,
}

#[derive(Deserialize, Debug)]
pub struct ChildAsset {
    child: String,
}
