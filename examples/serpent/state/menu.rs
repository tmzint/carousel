use crate::state::game::GameSetupState;
use crate::state::{RenderResources, SimResource, State};
use carousel::prelude::*;
use nalgebra::Point3;

#[derive(Default)]
pub struct MenuSetupState;

impl MenuSetupState {
    pub fn handler(
        h: OpenSimHandlerBuilder<MenuSetupState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<MenuSetupState, SimResource, State> {
        h.on(Self::on_frame_requested_event)
    }

    fn on_frame_requested_event(
        _state: &mut MenuSetupState,
        resources: &mut SimResources<SimResource>,
        _event: &FrameRequestedEvent,
    ) -> StateInstruction<State> {
        let render_resource = resources.resource.render.as_ref().unwrap();
        let menu_state = MenuState::new(render_resource);
        let menu_builder = resources.resource.menu_builder.clone();
        let menu = menu_builder.init_finish(resources, menu_state).unwrap();
        StateInstruction::pop_push(menu)
    }
}

pub struct MenuState {
    play_text: Text,
    exit_text: Text,
}

impl MenuState {
    pub fn new(render_resource: &RenderResources) -> Self {
        let play_text = render_resource.ui_layer.spawn(
            Text::builder()
                .with_content("play")
                .with_point(32.0)
                .with_width(128.0)
                .with_height(64.0)
                .with_position(Point3::new(0.0, 32.0, 0.1)),
        );

        let exit_text = render_resource.ui_layer.spawn(
            Text::builder()
                .with_content("exit")
                .with_point(32.0)
                .with_width(128.0)
                .with_height(64.0)
                .with_position(Point3::new(0.0, -32.0, 0.0)),
        );

        MenuState {
            play_text,
            exit_text,
        }
    }

    pub fn handler(
        h: OpenSimHandlerBuilder<MenuState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<MenuState, SimResource, State> {
        h.on(Self::on_pointer_input_event)
    }

    fn on_pointer_input_event(
        state: &mut MenuState,
        resources: &mut SimResources<SimResource>,
        event: &PointerInputEvent,
    ) -> StateInstruction<State> {
        let render_resource = resources.resource.render.as_ref().unwrap();
        let primary_release = event.ended(PointerKind::Primary);
        let world_cursor = event.cursor.to_world(&render_resource.ui_camera);

        if !primary_release {
            return StateInstruction::Stay;
        }

        if world_cursor.contained(&state.play_text) {
            let game_setup_builder = resources.resource.game_setup_builder.clone();
            let game_setup = game_setup_builder
                .init_finish(resources, GameSetupState::default())
                .unwrap();
            return StateInstruction::pop_push(game_setup);
        }

        if world_cursor.contained(&state.exit_text) {
            return StateInstruction::Pop;
        }

        StateInstruction::Stay
    }
}
