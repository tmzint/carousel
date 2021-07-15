use crate::state::menu::MenuSetupState;
use crate::state::{RenderResources, SimResource, State};
use crate::{LOGICAL_HEIGHT, LOGICAL_WIDTH};
use carousel::prelude::*;
use nalgebra::{Point3, Vector2};

#[derive(Default)]
pub struct SetupState;

impl SetupState {
    pub fn handler(
        h: OpenSimHandlerBuilder<SetupState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<SetupState, SimResource, State> {
        h.on(Self::on_frame_requested_event)
    }

    fn on_frame_requested_event(
        _state: &mut SetupState,
        resources: &mut SimResources<SimResource>,
        _event: &FrameRequestedEvent,
    ) -> StateInstruction<State> {
        let render = &resources.render;
        let camera_rect = Vector2::new(LOGICAL_WIDTH as f32, LOGICAL_HEIGHT as f32);
        let camera_eye = Point3::new(0.0, 0.0, 10000.0);

        let main_camera = render.camera(camera_rect, camera_eye);
        let main_layer = render.layer();

        let ui_camera = render.camera(camera_rect, camera_eye);
        let ui_layer = render.layer();

        let canvas = render
            .canvas_frame()
            .cover_layer(&main_layer, &main_camera, [0.0, 0.0, 0.0, 1.0])
            .stack_layer(&ui_layer, &ui_camera)
            .finish();

        resources.resource.render = Some(RenderResources {
            main_camera,
            main_layer,
            ui_camera,
            ui_layer,
            canvas,
        });

        let menu_setup = MenuSetupState::default();
        let menu_setup_builder = resources.resource.menu_setup_builder.clone();
        StateInstruction::pop_push(
            menu_setup_builder
                .init_finish(resources, menu_setup)
                .unwrap(),
        )
    }
}
