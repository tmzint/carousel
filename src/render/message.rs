use crate::asset::Weak;
use crate::render::camera::RawCamera;
use crate::render::canvas::{CanvasFrame, RawInstance};
use crate::render::client::{RenderClient, RenderDefaults};
use crate::render::curve::RawCurve;
use crate::render::text::RawText;
use parking_lot::Mutex;
use roundabout::prelude::MessageSender;
use std::ops::Deref;
use std::rc::Rc;
use uuid::Uuid;

#[derive(Debug)]
pub struct RenderCreatedEvent {
    pub defaults: Box<RenderDefaults>,
}

impl RenderCreatedEvent {
    #[inline]
    pub fn render_client(&self, sender: MessageSender) -> RenderClient {
        RenderClient::new(Rc::new(self.defaults.deref().to_owned()), sender)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DrawnEvent {
    pub frame: u64,
}

#[derive(Debug)]
pub struct CameraEvent {
    pub id: Uuid,
    pub raw: RawCamera,
    pub kind: CameraEventKind,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CameraEventKind {
    Created,
    Modified,
    Dropped,
}

#[derive(Debug)]
pub struct CanvasLayerEvent {
    pub id: Uuid,
    pub kind: CanvasLayerEventKind,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CanvasLayerEventKind {
    Created,
    Dropped,
}

#[derive(Debug)]
pub struct CanvasEvent {
    pub id: Uuid,
    pub kind: CanvasEventKind,
}

#[derive(Debug)]
pub enum CanvasEventKind {
    Created(Mutex<Option<CanvasEventCreated>>),
    Dropped,
}

#[derive(Debug)]
pub struct CanvasEventCreated {
    pub size: Option<[u32; 2]>,
    pub priority: usize,
    pub frame: bool,
    pub frames: Vec<CanvasFrame<'static>>,
}

#[derive(Debug)]
pub struct InstanceEvent {
    pub id: Uuid,
    pub layer: Uuid,
    pub kind: InstanceEventKind,
}

#[derive(Debug)]
pub enum InstanceEventKind {
    Created(Box<RawInstance<Weak>>),
    Modified(Box<RawInstance<Weak>>),
    Dropped,
}

#[derive(Debug)]
pub struct TextEvent {
    pub id: Uuid,
    pub layer: Uuid,
    pub kind: TextEventKind,
}

#[derive(Debug)]
pub enum TextEventKind {
    Created(Box<RawText<Weak>>),
    Modified {
        raw: Box<RawText<Weak>>,
        major_change: bool,
    },
    Dropped,
}

#[derive(Debug)]
pub struct CurveEvent {
    pub id: Uuid,
    pub layer: Uuid,
    pub kind: CurveEventKind,
}

#[derive(Debug)]
pub enum CurveEventKind {
    Created(Box<RawCurve<Weak>>),
    Modified {
        raw: Box<RawCurve<Weak>>,
        major_change: bool,
    },
    Dropped,
}
