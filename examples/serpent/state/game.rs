use crate::state::menu::MenuSetupState;
use crate::state::{RenderResources, SimResource, State};
use carousel::prelude::*;
use nalgebra::{Point2, Point3, Translation2, Vector2};
use rand::prelude::SliceRandom;
use std::time::Duration;

const GRID_ENTRY_EXTENDS: f32 = 64.0;
const GRID_HALF_EXTENDS: f32 = GRID_ENTRY_EXTENDS * 5.0;

#[derive(Default)]
pub struct GameSetupState;

impl GameSetupState {
    pub fn handler(
        h: OpenSimHandlerBuilder<GameSetupState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<GameSetupState, SimResource, State> {
        h.on(Self::on_frame_requested_event)
    }

    fn on_frame_requested_event(
        _state: &mut GameSetupState,
        resources: &mut SimResources<SimResource>,
        _event: &FrameRequestedEvent,
    ) -> StateInstruction<State> {
        let render_resource = resources.resource.render.as_ref().unwrap();

        let speed = 0.19;
        TimeServer::schedule(
            Duration::from_secs_f32(speed),
            AdvanceSerpentCommand,
            resources.context.sender(),
        );

        let game_run = GameRunState::new(speed, render_resource);
        let game_run_builder = resources.resource.game_run_builder.clone();
        StateInstruction::pop_push(game_run_builder.init_finish(resources, game_run).unwrap())
    }
}

pub struct AdvanceSerpentCommand;

#[allow(dead_code)]
pub struct SerpentSegment {
    position: Point2<isize>,
    rect: Rectangle,
}

impl SerpentSegment {
    pub fn new(position: Point2<isize>, render_resource: &RenderResources) -> Self {
        let logical = grid_point_to_logical(position);
        let rect = render_resource.main_layer.spawn(
            Rectangle::builder()
                .with_position(Point3::new(logical.x, logical.y, 0.0))
                .with_size(Vector2::new(GRID_ENTRY_EXTENDS, GRID_ENTRY_EXTENDS) * 0.8)
                .with_tint([1.0, 1.0, 1.0]),
        );

        SerpentSegment { position, rect }
    }
}

#[allow(dead_code)]
pub struct Food {
    position: Point2<isize>,
    rect: Rectangle,
}

impl Food {
    pub fn new(position: Point2<isize>, render_resource: &RenderResources) -> Self {
        let logical = grid_point_to_logical(position);
        let rect = render_resource.main_layer.spawn(
            Rectangle::builder()
                .with_position(Point3::new(logical.x, logical.y, 0.0))
                .with_size(Vector2::new(GRID_ENTRY_EXTENDS, GRID_ENTRY_EXTENDS) * 0.5)
                .with_tint([1.0, 1.0, 1.0]),
        );

        Food { position, rect }
    }
}

fn grid_point_to_logical(point: Point2<isize>) -> Point2<f32> {
    const TRANSLATION: f32 = GRID_ENTRY_EXTENDS / 2.0 - GRID_HALF_EXTENDS;
    Point2::new(
        point.x as f32 * GRID_ENTRY_EXTENDS + TRANSLATION,
        point.y as f32 * GRID_ENTRY_EXTENDS + TRANSLATION,
    )
}

#[allow(dead_code)]
pub struct GameRunState {
    grid_curve: Curve,
    prev_direction: Vector2<isize>,
    next_direction: Vector2<isize>,
    food: Food,
    food_list: Vec<Point2<isize>>,
    serpent: Vec<SerpentSegment>,
    speed: f32,
}

impl GameRunState {
    pub fn new(speed: f32, render_resource: &RenderResources) -> Self {
        let mut grid_path = Path::builder();
        for i in 0..11 {
            let i_extends = (i - 5) as f32 * GRID_ENTRY_EXTENDS;
            grid_path = grid_path
                .begin(Point2::new(-GRID_HALF_EXTENDS, i_extends))
                .line(Point2::new(GRID_HALF_EXTENDS, i_extends))
                .end()
                .begin(Point2::new(i_extends, -GRID_HALF_EXTENDS))
                .line(Point2::new(i_extends, GRID_HALF_EXTENDS))
                .end()
        }

        let grid_curve = render_resource.main_layer.spawn(
            Curve::builder()
                .with_path(grid_path.finalize())
                .with_tint([1.0, 1.0, 1.0])
                .with_stroke(StrokeOptions {
                    line_width: 3.0,
                    end_cap: LineCap::Round,
                    start_cap: LineCap::Round,
                    ..Default::default()
                }),
        );

        let direction = Vector2::new(1, 0);
        let food = Food::new(Point2::new(7, 7), render_resource);
        let food_list = Vec::new();
        let serpent_head = SerpentSegment::new(Point2::new(1, 3), render_resource);

        Self {
            grid_curve,
            next_direction: direction,
            prev_direction: direction,
            food,
            food_list,
            serpent: vec![serpent_head],
            speed,
        }
    }

    pub fn handler(
        h: OpenSimHandlerBuilder<GameRunState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<GameRunState, SimResource, State> {
        h.on(Self::on_action_event)
            .on(Self::on_advance_serpent_command)
    }

    fn on_action_event(
        state: &mut GameRunState,
        _resources: &mut SimResources<SimResource>,
        event: &ActionEvent,
    ) -> StateInstruction<State> {
        let new_direction = match event.name.as_str() {
            "MoveUp" => Vector2::new(0, 1),
            "MoveLeft" => Vector2::new(-1, 0),
            "MoveDown" => Vector2::new(0, -1),
            "MoveRight" => Vector2::new(1, 0),
            _ => {
                return StateInstruction::Stay;
            }
        };

        if state.prev_direction != -new_direction {
            state.next_direction = new_direction;
        }

        StateInstruction::Stay
    }

    fn on_advance_serpent_command(
        state: &mut GameRunState,
        resources: &mut SimResources<SimResource>,
        _event: &AdvanceSerpentCommand,
    ) -> StateInstruction<State> {
        let render_resource = resources.resource.render.as_ref().unwrap();

        let head_position = state.serpent.first().unwrap().position;
        let next_head_position = Translation2::from(state.next_direction) * head_position;
        state.prev_direction = state.next_direction;

        let hit_food = state.food.position == next_head_position;
        if !hit_food {
            state.serpent.pop();
        }

        let hit_self = state
            .serpent
            .iter()
            .any(|s| s.position == next_head_position);
        let hit_edge = next_head_position.x < 0
            || next_head_position.x >= 10
            || next_head_position.y < 0
            || next_head_position.y >= 10;

        if let Some(tail_head) = state.serpent.first_mut() {
            tail_head.rect.modify().scale = Vector2::new(0.9, 0.9);
        }
        state
            .serpent
            .insert(0, SerpentSegment::new(next_head_position, render_resource));
        let max_len = state.serpent.len() >= 100;

        if max_len || hit_self || hit_edge {
            let game_over = GameOverState::new(state.serpent.len(), render_resource);
            let game_over_builder = resources.resource.game_over_builder.clone();
            return StateInstruction::pop_push(
                game_over_builder.init_finish(resources, game_over).unwrap(),
            );
        }

        if hit_food {
            state.food = loop {
                if let Some(position) = state.food_list.pop() {
                    if !state.serpent.iter().any(|s| s.position == position) {
                        break Food::new(position, render_resource);
                    }
                } else {
                    for i in 0..100 {
                        state.food_list.push(Point2::new(i / 10, i % 10));
                    }
                    state.food_list.shuffle(&mut rand::thread_rng());
                }
            };
        }

        TimeServer::schedule(
            Duration::from_secs_f32(state.speed),
            AdvanceSerpentCommand,
            resources.context.sender(),
        );

        StateInstruction::Stay
    }
}

#[allow(dead_code)]
pub struct GameOverState {
    game_over_text: Text,
    score_text: Text,
    restart_text: Text,
    exit_text: Text,
}

impl GameOverState {
    pub fn new(points: usize, render_resource: &RenderResources) -> Self {
        let game_over_text = render_resource.ui_layer.spawn(
            Text::builder()
                .with_content("game over")
                .with_point(32.0)
                .with_position(Point3::new(0.0, 96.0, 0.0)),
        );

        let score_text = render_resource.ui_layer.spawn(
            Text::builder()
                .with_content(format!("{} points", points))
                .with_point(22.0)
                .with_position(Point3::new(0.0, 48.0, 0.0)),
        );

        let restart_text = render_resource.ui_layer.spawn(
            Text::builder()
                .with_content("restart")
                .with_point(22.0)
                .with_width(128.0)
                .with_height(44.0)
                .with_position(Point3::new(0.0, -32.0, 0.0)),
        );

        let exit_text = render_resource.ui_layer.spawn(
            Text::builder()
                .with_content("exit")
                .with_point(22.0)
                .with_width(128.0)
                .with_height(44.0)
                .with_position(Point3::new(0.0, -76.0, 0.0)),
        );

        Self {
            game_over_text,
            score_text,
            restart_text,
            exit_text,
        }
    }

    pub fn handler(
        h: OpenSimHandlerBuilder<GameOverState, SimResource, State>,
    ) -> OpenSimHandlerBuilder<GameOverState, SimResource, State> {
        h.on(Self::on_pointer_input_event)
    }

    fn on_pointer_input_event(
        state: &mut GameOverState,
        resources: &mut SimResources<SimResource>,
        event: &PointerInputEvent,
    ) -> StateInstruction<State> {
        let render_resource = resources.resource.render.as_ref().unwrap();
        let primary_release = event.ended(PointerKind::Primary);
        let world_cursor = event.cursor.to_world(&render_resource.ui_camera);

        if !primary_release {
            return StateInstruction::Stay;
        }

        if world_cursor.contained(&state.restart_text) {
            let game_setup_builder = resources.resource.game_setup_builder.clone();
            let game_setup = game_setup_builder
                .init_finish(resources, GameSetupState::default())
                .unwrap();
            return StateInstruction::pop_push(game_setup);
        }

        if world_cursor.contained(&state.exit_text) {
            let menu_setup_builder = resources.resource.menu_setup_builder.clone();
            let menu_setup = menu_setup_builder
                .init_finish(resources, MenuSetupState::default())
                .unwrap();
            return StateInstruction::pop_push(menu_setup);
        }

        StateInstruction::Stay
    }
}
