use crate::platform::action::ActionState;
use crate::platform::input::{Cursor, MouseButton, PointerKind, ScrollDirection};
use crate::platform::key::ScanCode;
use internment::Intern;
use parking_lot::Mutex;
use std::time::{Duration, Instant};

pub struct DisplayCreatedEvent {
    pub window_size: [u32; 2],
    pub render_resources: Mutex<Option<DisplayRenderResources>>,
}

impl DisplayCreatedEvent {
    pub fn new(
        window_size: [u32; 2],
        instance: wgpu::Instance,
        window_surface: wgpu::Surface,
    ) -> Self {
        let render_resources = Mutex::new(Some(DisplayRenderResources {
            instance,
            window_surface,
        }));

        Self {
            window_size,
            render_resources,
        }
    }
}

pub struct DisplayRenderResources {
    pub instance: wgpu::Instance,
    pub window_surface: wgpu::Surface,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameRequestedEvent {
    pub frame: u64,
    pub at: Instant,
    pub elapsed: Duration,
    pub delta: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct SuspendedEvent {
    pub at: Instant,
}

#[derive(Debug, Clone, Copy)]
pub struct ResumedEvent {
    pub at: Instant,
}

#[derive(Debug, Clone, Copy)]
pub struct DisplayResizedEvent {
    pub size: [u32; 2],
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MouseInputEvent {
    pub button: MouseButton,
    pub value: f32,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ScrollInputEvent {
    pub direction: ScrollDirection,
    pub value: f32,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CursorInputEvent {
    pub cursor: Cursor,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PointerInputEvent {
    pub id: u64,
    pub kind: PointerKind,
    pub cursor: Cursor,
    pub value: f32,
}

impl PointerInputEvent {
    #[inline]
    pub fn to_kind(&self, kind: PointerKind) -> Option<f32> {
        if self.kind == kind {
            Some(self.value)
        } else {
            None
        }
    }

    #[inline]
    pub fn ended(&self, kind: PointerKind) -> bool {
        self.kind == kind && self.value.abs() <= f32::EPSILON
    }

    #[inline]
    pub fn started(&self, kind: PointerKind) -> bool {
        // We currently only send started / ended events as we only support mouse
        self.kind == kind && self.value.abs() > f32::EPSILON
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct KeyInputEvent {
    pub scan: ScanCode,
    pub value: f32,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ActionEvent {
    pub name: Intern<String>,
    pub state: ActionState,
    pub value: f32,
}
