pub mod action;
pub mod input;
pub mod key;
pub mod message;

use crate::asset::storage::{Assets, AssetsClient};
use crate::asset::{
    AssetEvent, AssetEventKind, AssetPath, AssetsCreatedEvent, StrongAssetId, WeakAssetId,
};
use crate::platform::action::{Actions, ActionsConfig};
use crate::platform::input::Inputs;
use crate::platform::message::{
    DisplayCreatedEvent, DisplayResizedEvent, FrameRequestedEvent, ResumedEvent, SuspendedEvent,
};
use crate::render::message::DrawnEvent;
use crate::sim::SimulatedEvent;
use crate::InitEvent;
use roundabout::prelude::*;
use serde::Deserialize;
use std::time::Instant;
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DisplayConfig {
    pub title: String,
    pub resizable: bool,
    pub size: [u32; 2],
    pub maximized: bool,
    pub fullscreen: Fullscreen,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            title: "".to_string(),
            resizable: false,
            size: [1280, 720],
            maximized: false,
            fullscreen: Fullscreen::Windowed,
        }
    }
}

#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fullscreen {
    Windowed,
    Borderless,
}

impl Default for Fullscreen {
    fn default() -> Self {
        Fullscreen::Windowed
    }
}

impl Into<Option<winit::window::Fullscreen>> for Fullscreen {
    fn into(self) -> Option<winit::window::Fullscreen> {
        match self {
            Fullscreen::Windowed => None,
            Fullscreen::Borderless => Some(winit::window::Fullscreen::Borderless(None)),
        }
    }
}

enum ConfigOriginInner<T> {
    Inline(T),
    AssetPath(AssetPath),
    AssetId(StrongAssetId<T>),
}

// TODO: priority? User => Asset
pub struct ConfigOrigin<T> {
    inner: ConfigOriginInner<T>,
    dirty: bool,
}

impl<T: Send + Sync + 'static> ConfigOrigin<T> {
    fn init(&mut self, assets: &AssetsClient) {
        if let ConfigOriginInner::AssetPath(path) = &self.inner {
            let asset_id = assets.load(*path);
            self.dirty = assets.has(&asset_id);
            self.inner = ConfigOriginInner::AssetId(asset_id);
        }
    }

    fn take<'a>(&'a mut self, assets: &'a AssetsClient) -> Option<&'a T> {
        self.dirty = false;
        match &self.inner {
            ConfigOriginInner::Inline(config) => Some(config),
            ConfigOriginInner::AssetPath(_) => None,
            ConfigOriginInner::AssetId(id) => assets.try_get(id),
        }
    }

    fn dirty(&mut self, other: &WeakAssetId<T>) {
        if let ConfigOriginInner::AssetId(id) = &self.inner {
            if id.is_same_asset(other) {
                self.dirty = true;
            }
        }
    }
}

impl From<DisplayConfig> for ConfigOrigin<DisplayConfig> {
    fn from(t: DisplayConfig) -> Self {
        ConfigOrigin {
            inner: ConfigOriginInner::Inline(t),
            dirty: true,
        }
    }
}

impl From<ActionsConfig> for ConfigOrigin<ActionsConfig> {
    fn from(t: ActionsConfig) -> Self {
        ConfigOrigin {
            inner: ConfigOriginInner::Inline(t),
            dirty: true,
        }
    }
}

impl<T> From<AssetPath> for ConfigOrigin<T> {
    fn from(asset_path: AssetPath) -> Self {
        ConfigOrigin {
            inner: ConfigOriginInner::AssetPath(asset_path),
            dirty: false,
        }
    }
}

pub struct PlatformServer {
    display_config: ConfigOrigin<DisplayConfig>,
    actions_config: ConfigOrigin<ActionsConfig>,
    start: Instant,
    curr: Instant,
    requested_frame: u64,
    drawn_frame: u64,
    simulated_frame: u64,
    assets: Option<Assets>,
}

impl PlatformServer {
    /**
    The Platform Server need to run on the primary thread
    */
    pub fn new<DC: Into<ConfigOrigin<DisplayConfig>>, AC: Into<ConfigOrigin<ActionsConfig>>>(
        display_config: DC,
        actions_config: AC,
        mut group: MessageGroupBuilder,
    ) -> MessageGroup {
        let display_config = display_config.into();
        let actions_config = actions_config.into();

        let platform_builder = group.register(|h| {
            h.on(on_init_event)
                .on(on_asset_created_event)
                .on(on_display_config_loaded_event)
                .on(on_actions_config_loaded_event)
                .on(on_drawn_event)
                .on(on_simulated_event)
                .init_fn(|_| {
                    let start = Instant::now();
                    PlatformServer {
                        display_config,
                        actions_config,
                        start,
                        curr: start,
                        requested_frame: 0,
                        drawn_frame: 0,
                        simulated_frame: 0,
                        assets: None,
                    }
                })
        });

        group.init(move |recv, context| {
            let platform = platform_builder.finish(&context).unwrap();
            run_event_loop(platform, recv, context);
        })
    }
}

fn on_init_event(state: &mut PlatformServer, _context: &mut RuntimeContext, event: &InitEvent) {
    state.start = event.start;
    state.curr = event.start;
}

fn on_asset_created_event(
    state: &mut PlatformServer,
    context: &mut RuntimeContext,
    event: &AssetsCreatedEvent,
) {
    assert!(state.assets.is_none());
    state.assets = Some(event.assets(context.sender().to_owned()));

    let asset = state.assets.as_mut().unwrap().client();
    state.display_config.init(&asset);
    state.actions_config.init(&asset);
}

fn on_display_config_loaded_event(
    state: &mut PlatformServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<DisplayConfig>,
) {
    if AssetEventKind::Load == event.kind {
        state.display_config.dirty(&event.id);
    }
}

fn on_actions_config_loaded_event(
    state: &mut PlatformServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<ActionsConfig>,
) {
    if AssetEventKind::Load == event.kind {
        state.actions_config.dirty(&event.id);
    }
}

fn on_drawn_event(state: &mut PlatformServer, _context: &mut RuntimeContext, event: &DrawnEvent) {
    state.drawn_frame = state.drawn_frame.max(event.frame);
}

fn on_simulated_event(
    state: &mut PlatformServer,
    _context: &mut RuntimeContext,
    event: &SimulatedEvent,
) {
    state.simulated_frame = state.simulated_frame.max(event.frame);
}

fn run_event_loop(
    mut platform: MessageHandler<PlatformServer>,
    mut recv: MessageReceiver,
    mut context: RuntimeContext,
) {
    // wait for config
    let wait_config_result = recv.recv_while(|message| {
        platform.handle(&mut context, message);
        !platform.state.display_config.dirty
    });
    if let Err(e) = wait_config_result {
        log::info!(
            "don't start platform server as runtime has already been shutdown: {}",
            e
        );
        return;
    }

    let window_builder = {
        let assets = platform.state.assets.as_mut().unwrap().client();
        let config = platform
            .state
            .display_config
            .take(&assets)
            .expect("loaded display config");

        log::info!("starting display with: {:?}", config);
        WindowBuilder::new()
            .with_title(&config.title)
            .with_resizable(config.resizable)
            .with_maximized(config.maximized)
            .with_fullscreen(config.fullscreen.into())
            .with_inner_size(PhysicalSize::new(config.size[0], config.size[1]))
    };

    let event_loop = EventLoop::new();
    let window = window_builder.build(&event_loop).unwrap();

    let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
    let window_surface = unsafe { instance.create_surface(&window) };
    let mut inputs = Inputs::new(window.inner_size().into());
    let mut actions = {
        // Optimization: move to a separate MessageHandler
        let assets = platform.state.assets.as_mut().unwrap().client();
        let config = platform.state.actions_config.take(&assets);
        Actions::new(config.cloned().unwrap_or_default())
    };

    context.sender().send(DisplayCreatedEvent::new(
        window.inner_size().into(),
        instance,
        window_surface,
    ));

    event_loop.run(move |event, _, control_flow| match event {
        Event::Suspended => {
            log::info!("suspended");
            context.sender().send(SuspendedEvent { at: Instant::now() });
        }
        Event::Resumed => {
            log::info!("resumed");
            context.sender().send(ResumedEvent { at: Instant::now() });
        }
        Event::MainEventsCleared => {
            if platform.state.display_config.dirty {
                let assets = platform.state.assets.as_mut().unwrap().client();
                let config = platform
                    .state
                    .display_config
                    .take(&assets)
                    .expect("loaded display config");

                log::info!("updating display with: {:?}", config);
                window.set_title(&config.title);
                window.set_resizable(config.resizable);
                window.set_maximized(config.maximized);
                window.set_fullscreen(config.fullscreen.into());
                window.set_inner_size(PhysicalSize::new(config.size[0], config.size[1]));
                inputs.set_cursor_rect(config.size);
                // required as a window size change here won't trigger the WindowEvent::Resized event
                context
                    .sender()
                    .send(DisplayResizedEvent { size: config.size });
            }

            if platform.state.actions_config.dirty {
                let assets = platform.state.assets.as_mut().unwrap().client();
                let config = platform.state.actions_config.take(&assets);
                actions.set_config(config.cloned().unwrap_or_default());
            }

            actions.push_inputs(&inputs);
            actions.apply_actions(context.sender());
            inputs.apply_inputs(context.sender());

            let frame_requested = {
                let at = Instant::now();
                let delta = at.duration_since(platform.state.curr);
                let elapsed = at.duration_since(platform.state.start);
                platform.state.curr = at;

                let frame = platform.state.requested_frame + 1;
                platform.state.requested_frame = frame;

                FrameRequestedEvent {
                    frame,
                    at,
                    elapsed,
                    delta,
                }
            };
            context.sender().send(frame_requested);

            // wait for the frame to be drawn and simulated
            let wait_frame_result = recv.recv_while(|message| {
                platform.handle(&mut context, message);
                let s = &platform.state;
                s.drawn_frame < s.requested_frame || s.simulated_frame < s.requested_frame
            });
            if let Err(e) = wait_frame_result {
                log::info!("shutdown platform server: {}", e);
                *control_flow = ControlFlow::Exit;
            }
        }
        Event::WindowEvent {
            ref event,
            window_id,
        } => {
            if window_id == window.id() {
                match event {
                    WindowEvent::Resized(size) => {
                        context.sender().send(DisplayResizedEvent {
                            size: (*size).into(),
                        });
                        inputs.set_cursor_rect((*size).into());
                    }
                    WindowEvent::CloseRequested => {
                        context.shutdown_switch().request_shutdown();
                    }
                    WindowEvent::Destroyed => {
                        context.shutdown_switch().request_shutdown();
                    }
                    unhandled => {
                        inputs.push_event(unhandled);
                    }
                }
            }
        }
        Event::LoopDestroyed => {
            log::info!("loop destroyed");
            context.shutdown_switch().request_shutdown();
        }
        _ => {}
    });
}
