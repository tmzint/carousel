mod base;
mod game;
mod menu;

use crate::state::base::SetupState;
use crate::state::game::{GameOverState, GameRunState, GameSetupState};
use crate::state::menu::{MenuSetupState, MenuState};
use carousel::prelude::*;

#[allow(dead_code)]
pub struct RenderResources {
    pub main_camera: Camera,
    pub main_layer: CanvasLayer,
    pub ui_camera: Camera,
    pub ui_layer: CanvasLayer,
    pub canvas: Canvas,
}

pub struct SimResource {
    pub setup_builder: ClosedSimHandlerBuilder<SetupState, SimResource, State>,
    pub menu_setup_builder: ClosedSimHandlerBuilder<MenuSetupState, SimResource, State>,
    pub menu_builder: ClosedSimHandlerBuilder<MenuState, SimResource, State>,
    pub game_setup_builder: ClosedSimHandlerBuilder<GameSetupState, SimResource, State>,
    pub game_run_builder: ClosedSimHandlerBuilder<GameRunState, SimResource, State>,
    pub game_over_builder: ClosedSimHandlerBuilder<GameOverState, SimResource, State>,
    pub render: Option<RenderResources>,
}

impl SimResource {
    pub fn init_fn(group: &mut MessageGroupBuilder) -> Box<dyn FnOnce() -> Self + Send + 'static> {
        let setup_builder = group.register(SetupState::handler);
        let menu_setup_builder = group.register(MenuSetupState::handler);
        let menu_builder = group.register(MenuState::handler);
        let game_setup_builder = group.register(GameSetupState::handler);
        let game_run_builder = group.register(GameRunState::handler);
        let game_over_builder = group.register(GameOverState::handler);

        Box::new(|| SimResource {
            setup_builder,
            menu_setup_builder,
            menu_builder,
            game_setup_builder,
            game_run_builder,
            game_over_builder,
            render: None,
        })
    }
}

pub enum State {
    Setup(SimHandler<SetupState, SimResource, State>),
    MenuSetup(SimHandler<MenuSetupState, SimResource, State>),
    Menu(SimHandler<MenuState, SimResource, State>),
    GameSetup(SimHandler<GameSetupState, SimResource, State>),
    GameRun(SimHandler<GameRunState, SimResource, State>),
    GameOver(SimHandler<GameOverState, SimResource, State>),
}

impl State {
    pub fn new(resources: &SimResources<SimResource>) -> State {
        resources
            .resource
            .setup_builder
            .clone()
            .init_finish(resources, SetupState::default())
            .unwrap()
            .into()
    }
}

impl SimState<SimResource> for State {
    fn handle<M: MessageView>(
        &mut self,
        resources: &mut SimResources<SimResource>,
        message: &M,
    ) -> Option<StateInstruction<Self>> {
        match self {
            State::Setup(s) => s.handle(resources, message),
            State::MenuSetup(s) => s.handle(resources, message),
            State::Menu(s) => s.handle(resources, message),
            State::GameSetup(s) => s.handle(resources, message),
            State::GameRun(s) => s.handle(resources, message),
            State::GameOver(s) => s.handle(resources, message),
        }
    }
}

impl From<SimHandler<SetupState, SimResource, State>> for State {
    fn from(s: SimHandler<SetupState, SimResource, State>) -> Self {
        Self::Setup(s)
    }
}

impl From<SimHandler<MenuSetupState, SimResource, State>> for State {
    fn from(s: SimHandler<MenuSetupState, SimResource, State>) -> Self {
        Self::MenuSetup(s)
    }
}

impl From<SimHandler<MenuState, SimResource, State>> for State {
    fn from(s: SimHandler<MenuState, SimResource, State>) -> Self {
        Self::Menu(s)
    }
}

impl From<SimHandler<GameSetupState, SimResource, State>> for State {
    fn from(s: SimHandler<GameSetupState, SimResource, State>) -> Self {
        Self::GameSetup(s)
    }
}

impl From<SimHandler<GameRunState, SimResource, State>> for State {
    fn from(s: SimHandler<GameRunState, SimResource, State>) -> Self {
        Self::GameRun(s)
    }
}

impl From<SimHandler<GameOverState, SimResource, State>> for State {
    fn from(s: SimHandler<GameOverState, SimResource, State>) -> Self {
        Self::GameOver(s)
    }
}
