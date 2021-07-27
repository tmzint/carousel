use crate::platform::key::ScanCode;
use crate::platform::message::{
    CursorInputEvent, KeyInputEvent, MouseInputEvent, PointerInputEvent, ScrollInputEvent,
};
use crate::prelude::{Camera, MessageSender};
use crate::util::Bounded;
use nalgebra::{Point2, Vector2};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum ScrollDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum PointerKind {
    Primary,
    Secondary,
    Tertiary,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum InputEvent {
    Mouse(MouseInputEvent),
    Scroll(ScrollInputEvent),
    Key(KeyInputEvent),
    Cursor(CursorInputEvent),
}

pub struct Inputs {
    buffer: Vec<InputEvent>,
    cursor: Cursor,
    raw_cursor: Option<Vector2<f64>>,
    cursor_left: bool,
    cursor_rect: [u32; 2],
}

impl Inputs {
    pub(crate) fn new(cursor_rect: [u32; 2]) -> Self {
        Self {
            buffer: Default::default(),
            cursor: Cursor::empty(cursor_rect),
            raw_cursor: None,
            cursor_left: false,
            cursor_rect,
        }
    }

    pub(crate) fn set_cursor_rect(&mut self, cursor_rect: [u32; 2]) {
        self.cursor_rect = cursor_rect;
        self.cursor = self.cursor.with_cursor_rect(cursor_rect);
    }

    pub(crate) fn queued_events(&self) -> &[InputEvent] {
        &self.buffer
    }

    pub(crate) fn push_event(&mut self, event: &winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::DroppedFile(_) => {}
            winit::event::WindowEvent::HoveredFile(_) => {}
            winit::event::WindowEvent::HoveredFileCancelled => {}
            winit::event::WindowEvent::ReceivedCharacter(_) => {}
            winit::event::WindowEvent::Focused(_) => {}
            winit::event::WindowEvent::KeyboardInput { input, .. } => {
                // TODO: this is completely broken (e.g. arrows), fix with https://github.com/rust-windowing/winit/issues/1806
                match ScanCode::from_windows(input.scancode as u8) {
                    Some(scan) => {
                        let value = match input.state {
                            winit::event::ElementState::Pressed => 1.0,
                            winit::event::ElementState::Released => 0.0,
                        };
                        self.buffer
                            .push(InputEvent::Key(KeyInputEvent { scan, value }));
                    }
                    None => {
                        log::debug!("ignoring unknown scancode of: {}", input.scancode);
                    }
                };
            }
            winit::event::WindowEvent::ModifiersChanged(_) => {}
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                // we can't push a cursor input here as we don't have access
                // to a possible updated size for the raw position transformation
                self.raw_cursor = Some(Vector2::new(position.x, position.y));
            }
            winit::event::WindowEvent::CursorEntered { .. } => {
                self.cursor_left = false;
            }
            winit::event::WindowEvent::CursorLeft { .. } => {
                self.cursor_left = true;
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => match delta {
                winit::event::MouseScrollDelta::LineDelta(h, v) => {
                    if h.abs() > f32::EPSILON {
                        self.buffer.push(InputEvent::Scroll(ScrollInputEvent {
                            direction: ScrollDirection::Horizontal,
                            value: *h,
                        }));
                    }
                    if v.abs() > f32::EPSILON {
                        self.buffer.push(InputEvent::Scroll(ScrollInputEvent {
                            direction: ScrollDirection::Vertical,
                            value: *v,
                        }));
                    }
                }
                winit::event::MouseScrollDelta::PixelDelta(_) => {}
            },
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    winit::event::MouseButton::Other(i) => MouseButton::Other(*i),
                };
                let value = match state {
                    winit::event::ElementState::Pressed => 1.0,
                    winit::event::ElementState::Released => 0.0,
                };
                self.buffer
                    .push(InputEvent::Mouse(MouseInputEvent { button, value }));
            }
            winit::event::WindowEvent::TouchpadPressure { .. } => {}
            winit::event::WindowEvent::AxisMotion { .. } => {}
            winit::event::WindowEvent::Touch(_) => {}
            _ => {}
        }
    }

    pub(crate) fn apply_inputs(&mut self, sender: &MessageSender) {
        if self.cursor_left {
            self.raw_cursor = None;
            self.cursor = Cursor::empty(self.cursor_rect);
            self.buffer.push(InputEvent::Cursor(CursorInputEvent {
                cursor: self.cursor,
            }));
        } else if let Some(raw_cursor) = self.raw_cursor.take() {
            self.cursor = Cursor::new(raw_cursor, self.cursor_rect);
            self.buffer.push(InputEvent::Cursor(CursorInputEvent {
                cursor: self.cursor,
            }));
        }

        for input in self.buffer.drain(..) {
            match input {
                InputEvent::Mouse(event) => {
                    sender.send(event);

                    let pointer_kind = match event.button {
                        MouseButton::Left => PointerKind::Primary,
                        MouseButton::Right => PointerKind::Secondary,
                        MouseButton::Middle => PointerKind::Tertiary,
                        MouseButton::Other(_) => {
                            continue;
                        }
                    };

                    sender.send(PointerInputEvent {
                        id: 0,
                        kind: pointer_kind,
                        cursor: self.cursor,
                        value: event.value,
                    });
                }
                InputEvent::Scroll(event) => {
                    sender.send(event);
                }
                InputEvent::Key(event) => {
                    sender.send(event);
                }
                InputEvent::Cursor(event) => {
                    sender.send(event);
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Cursor {
    raw_transform: Option<Vector2<f64>>,
    rel_transform: Option<Vector2<f64>>,
    cursor_rect: [u32; 2],
}

impl Cursor {
    fn new<I: Into<Option<Vector2<f64>>>>(transform: I, cursor_rect: [u32; 2]) -> Self {
        let raw_transform = transform.into();
        let rel_transform = raw_transform.map(|raw| Self::relative_transform(raw, cursor_rect));

        Self {
            raw_transform,
            rel_transform,
            cursor_rect,
        }
    }

    fn empty(cursor_rect: [u32; 2]) -> Self {
        Self {
            raw_transform: None,
            rel_transform: None,
            cursor_rect,
        }
    }

    fn with_cursor_rect(mut self, cursor_rect: [u32; 2]) -> Self {
        self.cursor_rect = cursor_rect;
        self.rel_transform = self
            .raw_transform
            .map(|raw| Self::relative_transform(raw, cursor_rect));
        self
    }

    fn relative_transform(raw: Vector2<f64>, cursor_rect: [u32; 2]) -> Vector2<f64> {
        let normalized_x = (raw.x / cursor_rect[0] as f64).clamp(0.0, 1.0);
        let normalized_y = (raw.y / cursor_rect[1] as f64).clamp(0.0, 1.0);
        Vector2::new(normalized_x - 0.5, (normalized_y - 0.5) * -1.0)
    }

    #[inline]
    pub fn cursor_rect(&self) -> [u32; 2] {
        self.cursor_rect
    }

    #[inline]
    pub fn raw_transform(&self) -> Option<Vector2<f64>> {
        self.raw_transform
    }

    #[inline]
    pub fn rel_transform(&self) -> Option<Vector2<f64>> {
        self.rel_transform
    }

    #[inline]
    pub fn to_world(&self, camera: &Camera) -> WorldCursor {
        let world_transform = self.rel_transform.map(|v| {
            camera.relative_to_world(
                v,
                Vector2::new(self.cursor_rect[0] as f32, self.cursor_rect[1] as f32),
            )
        });

        WorldCursor {
            cursor: self,
            world_transform,
        }
    }
}

#[derive(Debug)]
pub struct WorldCursor<'a> {
    cursor: &'a Cursor,
    world_transform: Option<Point2<f64>>,
}

impl<'a> WorldCursor<'a> {
    #[inline]
    pub fn point(&self) -> Option<Point2<f64>> {
        self.world_transform
    }

    #[inline]
    pub fn contained<B: Bounded>(&self, bounds: &B) -> bool {
        self.world_transform
            .map(|point| bounds.contains(nalgebra::convert(point)))
            .unwrap_or_default()
    }
}

impl<'a> Deref for WorldCursor<'a> {
    type Target = Cursor;

    fn deref(&self) -> &Self::Target {
        self.cursor
    }
}
