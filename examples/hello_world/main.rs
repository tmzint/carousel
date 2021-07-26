use carousel::prelude::*;
use nalgebra::{Point2, Rotation2, Vector2};

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .filter_module("wgpu_core", log::LevelFilter::Warn)
        .init();

    let runtime = Runtime::builder(2_097_152)
        .add(RenderServer::new)
        .add(TimeServer::new)
        .add(|b| AssetServer::builder(b).finish())
        .add_group(|mut g| {
            let resource = SimResource {
                setup_builder: g.register(SetupState::handler),
                running_builder: g.register(RunningState::handler),
            };
            SimServer::builder(g, || resource).init_fn(|resources| {
                let setup_builder = resources.resource.setup_builder.clone();
                State::from(setup_builder.init_finish(resources, SetupState).unwrap())
            })
        })
        .finish_main_group(|g| {
            PlatformServer::new(
                AssetPath::sys("display.json"),
                AssetPath::sys("actions.json"),
                g,
            )
        });

    Engine::builder()?
        .with_sys_path("examples/hello_world/sys/")
        .with_runtime(runtime)
        .finish()
        .start()
}

pub struct SimResource {
    setup_builder: ClosedSimHandlerBuilder<SetupState, SimResource, State>,
    running_builder: ClosedSimHandlerBuilder<RunningState, SimResource, State>,
}

enum State {
    Setup(SimHandler<SetupState, SimResource, State>),
    Running(SimHandler<RunningState, SimResource, State>),
}

impl SimState<SimResource> for State {
    fn handle<M: MessageView>(
        &mut self,
        resources: &mut SimResources<SimResource>,
        message: &M,
    ) -> Option<StateInstruction<Self>> {
        match self {
            State::Setup(s) => s.handle(resources, message),
            State::Running(s) => s.handle(resources, message),
        }
    }
}

impl From<SimHandler<SetupState, SimResource, State>> for State {
    fn from(s: SimHandler<SetupState, SimResource, State>) -> Self {
        Self::Setup(s)
    }
}

impl From<SimHandler<RunningState, SimResource, State>> for State {
    fn from(s: SimHandler<RunningState, SimResource, State>) -> Self {
        Self::Running(s)
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
        let render = &resources.render;
        let camera_rect = Vector2::new(1280.0, 720.0);
        let camera_eye = Point2::new(0.0, 0.0);

        let main_camera = render.camera(camera_rect, camera_eye);
        let main_layer = render.layer();
        let sprite = main_layer.spawn(
            Sprite::builder()
                .with_size(Vector2::new(128.0, 128.0))
                .with_texture(assets.load(AssetPath::sys("hello_world.json"))),
        );

        let ui_camera = render.camera(camera_rect, camera_eye);
        let ui_layer = render.layer();
        let text = ui_layer.spawn(
            Text::builder()
                .with_content("Hello World!")
                .with_point(16.0)
                .with_position(Point2::new(-300.0, 0.0)),
        );

        let canvas = render
            .canvas_frame()
            .cover_layer(&main_layer, &main_camera, [0.0, 0.0, 0.0, 1.0])
            .stack_layer(&ui_layer, &ui_camera)
            .finish();

        std::mem::drop(assets);

        let running = RunningState {
            main_camera,
            main_layer,
            ui_camera,
            ui_layer,
            canvas,
            sprite,
            text,
        };

        let running_builder = resources.resource.running_builder.clone();
        StateInstruction::pop_push(running_builder.init_finish(resources, running).unwrap())
    }
}

#[allow(dead_code)]
struct RunningState {
    main_camera: Camera,
    main_layer: CanvasLayer,
    ui_camera: Camera,
    ui_layer: CanvasLayer,
    canvas: Canvas,
    sprite: Sprite,
    text: Text,
}

impl RunningState {
    fn handler(
        h: OpenSimHandlerBuilder<RunningState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<RunningState, SimResource, State> {
        h.on(Self::on_frame_requested_event)
    }

    fn on_frame_requested_event(
        state: &mut RunningState,
        _resources: &mut SimResources<SimResource>,
        event: &FrameRequestedEvent,
    ) -> StateInstruction<State> {
        let normalized_secs = event.elapsed.as_secs_f32() % 5.0;

        let mut sprite = state.sprite.modify();
        sprite.rotation = Rotation2::new(normalized_secs * std::f32::consts::PI);
        sprite.position.x = normalized_secs * 100.0;

        let mut text = state.text.modify();
        text.rotation = Rotation2::new(normalized_secs * -std::f32::consts::PI);
        text.scale = 1.0 + normalized_secs;
        if normalized_secs % 1.0 < 0.5 {
            if text.content.ends_with("?") {
                text.content = format!("{}!", text.content.strip_suffix("?").unwrap()).into();
            }
        } else {
            if text.content.ends_with("!") {
                text.content = format!("{}?", &text.content.strip_suffix("!").unwrap()).into();
            }
        }

        if event.frame % 1000 == 0 {
            println!("Sim-FrameRequested: {:?}", event);
        }

        StateInstruction::Stay
    }
}
