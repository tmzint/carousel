mod camera;
mod curve;
mod instance;
mod rectangle;
mod sprite;
mod text;

pub use camera::*;
pub use curve::*;
pub use instance::*;
pub use rectangle::*;
pub use sprite::*;
pub use text::*;

use crate::asset::StrongAssetId;
use crate::render::canvas::CanvasFrame;
use crate::render::mesh::{Mesh, Meshes};
use crate::render::message::{
    CanvasEvent, CanvasEventCreated, CanvasEventKind, CanvasLayerEvent, CanvasLayerEventKind,
};
use crate::render::pipeline::{Pipeline, Pipelines};
use crate::render::text::{Font, Texts};
use crate::render::view::{Texture, Textures};
use nalgebra::{Point2, Vector2, Vector3};
use parking_lot::Mutex;
use roundabout::prelude::MessageSender;
use std::marker::PhantomData;
use std::rc::Rc;
use uuid::Uuid;

//  Optimization: smaller change-sets on modify

#[derive(Clone)]
pub struct RenderClient {
    pub defaults: Rc<RenderDefaults>,
    sender: MessageSender,
}

impl RenderClient {
    #[inline]
    pub fn new(defaults: Rc<RenderDefaults>, sender: MessageSender) -> Self {
        Self { defaults, sender }
    }

    #[inline]
    pub fn camera(&self, rect: Vector2<f32>, eye: Point2<f32>) -> Camera {
        Camera::new(rect, eye, self.sender.clone())
    }

    #[inline]
    pub fn layer(&self) -> CanvasLayer {
        CanvasLayer::new(self.defaults.clone(), self.sender.clone())
    }

    #[inline]
    pub fn canvas_frame(&self) -> CanvasBuilder<FrameCanvas> {
        Canvas::frame(&self.sender)
    }

    #[inline]
    pub fn canvas_general(&self) -> CanvasBuilder<GeneralCanvas> {
        Canvas::general(&self.sender)
    }
}

#[derive(Debug, Clone)]
pub struct RenderDefaults {
    pub unlit_pipeline: StrongAssetId<Pipeline>,
    pub unlit_alpha_pipeline: StrongAssetId<Pipeline>,
    pub text_pipeline: StrongAssetId<Pipeline>,
    pub font: StrongAssetId<Font>,
    pub white_texture: StrongAssetId<Texture>,
    pub empty_mesh: StrongAssetId<Mesh>,
    pub unit_square_mesh: StrongAssetId<Mesh>,
}

impl RenderDefaults {
    pub(crate) fn new(
        pipelines: &Pipelines,
        textures: &Textures,
        meshes: &Meshes,
        texts: &Texts,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            unlit_pipeline: pipelines.unlit_pipeline.clone(),
            unlit_alpha_pipeline: pipelines.unlit_alpha_pipeline.clone(),
            text_pipeline: pipelines.text_pipeline.clone(),
            font: texts.font.clone(),
            white_texture: textures.white_texture.clone(),
            empty_mesh: meshes.empty_mesh.clone(),
            unit_square_mesh: meshes.unit_square_mesh.clone(),
        })
    }
}

pub trait LayerSpawner {
    type Handle;

    fn spawn(self, layer: &CanvasLayer) -> Self::Handle;
}

#[derive(Debug)]
pub struct CanvasLayer {
    id: Uuid,
    defaults: Rc<RenderDefaults>,
    pub(crate) sender: MessageSender,
}

impl CanvasLayer {
    pub(crate) fn new(defaults: Rc<RenderDefaults>, sender: MessageSender) -> Self {
        let id = Uuid::new_v4();

        sender.send(CanvasLayerEvent {
            id,
            kind: CanvasLayerEventKind::Created,
        });

        CanvasLayer {
            id,
            defaults,
            sender,
        }
    }

    #[inline]
    pub fn id(&self) -> Uuid {
        self.id
    }

    #[inline]
    pub fn defaults(&self) -> &RenderDefaults {
        &self.defaults
    }

    #[inline]
    pub fn spawn<T: LayerSpawner>(&self, spawner: T) -> T::Handle {
        spawner.spawn(self)
    }
}

impl Drop for CanvasLayer {
    #[inline]
    fn drop(&mut self) {
        self.sender.send(CanvasLayerEvent {
            id: self.id,
            kind: CanvasLayerEventKind::Dropped,
        });
    }
}

#[derive(Debug)]
pub struct FrameCanvas;

#[derive(Debug)]
pub struct GeneralCanvas;

#[derive(Debug)]
pub struct CanvasBuilder<'a, T> {
    size: Option<[u32; 2]>,
    frames: Vec<CanvasFrame<'a>>,
    priority: usize,
    frame: bool,
    sender: &'a MessageSender,
    _pd: PhantomData<T>,
}

impl<'a> CanvasBuilder<'a, GeneralCanvas> {
    #[inline]
    pub fn with_size(mut self, size: [u32; 2]) -> Self {
        self.size = Some(size);
        self
    }
}

impl<'a, T> CanvasBuilder<'a, T> {
    #[inline]
    pub fn with_priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }

    #[inline]
    pub fn cover_layer(
        mut self,
        layer: &'a CanvasLayer,
        camera: &'a Camera,
        clear_color: [f64; 4],
    ) -> Self {
        self.frames.push(CanvasFrame::Cover {
            layer: layer.id(),
            camera: camera.id(),
            clear_color,
            _pd: Default::default(),
        });
        self
    }

    #[inline]
    pub fn merge_layer(mut self, layer: &'a CanvasLayer, camera: &'a Camera) -> Self {
        self.frames.push(CanvasFrame::Merge {
            layer: layer.id(),
            camera: camera.id(),
            _pd: Default::default(),
        });
        self
    }

    #[inline]
    pub fn stack_layer(mut self, layer: &'a CanvasLayer, camera: &'a Camera) -> Self {
        self.frames.push(CanvasFrame::Stack {
            layer: layer.id(),
            camera: camera.id(),
            _pd: Default::default(),
        });
        self
    }

    #[inline]
    pub fn finish(self) -> Canvas {
        let id = Uuid::new_v4();

        let kind = CanvasEventKind::Created(Mutex::new(Some(CanvasEventCreated {
            size: self.size,
            priority: self.priority,
            frame: self.frame,
            frames: self
                .frames
                .into_iter()
                .map(CanvasFrame::into_static)
                .collect(),
        })));

        self.sender.send(CanvasEvent { id, kind });

        Canvas {
            id,
            size: self.size,
            sender: self.sender.to_owned(),
            priority: self.priority,
        }
    }
}

#[derive(Debug)]
pub struct Canvas {
    id: Uuid,
    size: Option<[u32; 2]>,
    priority: usize,
    sender: MessageSender,
}

impl Canvas {
    fn frame(sender: &MessageSender) -> CanvasBuilder<FrameCanvas> {
        CanvasBuilder {
            size: None,
            frames: Default::default(),
            priority: 0,
            frame: true,
            sender,
            _pd: Default::default(),
        }
    }

    fn general(sender: &MessageSender) -> CanvasBuilder<GeneralCanvas> {
        CanvasBuilder {
            size: None,
            frames: Default::default(),
            priority: 0,
            frame: false,
            sender,
            _pd: Default::default(),
        }
    }

    #[inline]
    pub fn id(&self) -> Uuid {
        self.id
    }
}

impl Drop for Canvas {
    #[inline]
    fn drop(&mut self) {
        self.sender.send(CanvasEvent {
            id: self.id,
            kind: CanvasEventKind::Dropped,
        });
    }
}

fn vector2_one() -> Vector2<f32> {
    Vector2::new(1.0, 1.0)
}

fn vector3_one() -> Vector3<f32> {
    Vector3::new(1.0, 1.0, 1.0)
}

fn f32_one() -> f32 {
    1.0
}

fn arr4_one() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
