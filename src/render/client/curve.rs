use crate::asset::{AssetId, Strong};
use crate::render::client::{CanvasLayer, LayerSpawner, RenderDefaults};
use crate::render::curve::{Path, RawCurve, StrokeOptions};
use crate::render::message::{CurveEvent, CurveEventKind};
use crate::render::pipeline::Pipeline;
use nalgebra::{Point2, Rotation2, Similarity2, Vector2};
use roundabout::prelude::MessageSender;
use serde::Deserialize;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveBuilder<S> {
    #[serde(default, bound(deserialize = "AssetId<Pipeline, S>: Deserialize<'de>"))]
    pub pipeline: Option<AssetId<Pipeline, S>>,
    #[serde(default)]
    pub path: Path,
    #[serde(default)]
    pub stroke: StrokeOptions,
    #[serde(default = "Point2::origin")]
    pub position: Point2<f32>,
    #[serde(default)]
    pub z_index: f32,
    #[serde(default = "Rotation2::identity")]
    pub rotation: Rotation2<f32>,
    #[serde(default = "super::vector2_one")]
    pub scale: Vector2<f32>,
    #[serde(default = "super::arr4_one")]
    pub tint: [f32; 4],
    #[serde(default = "Similarity2::identity")]
    pub world: Similarity2<f32>,
    #[serde(default)]
    pub world_z_index: f32,
    #[serde(default)]
    pub hidden: bool,
}

impl<S> CurveBuilder<S> {
    #[inline]
    pub fn with_pipeline(mut self, pipeline: AssetId<Pipeline, S>) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    #[inline]
    pub fn with_path(mut self, path: Path) -> Self {
        self.path = path;
        self
    }

    #[inline]
    pub fn with_stroke(mut self, stroke: StrokeOptions) -> Self {
        self.stroke = stroke;
        self
    }

    #[inline]
    pub fn with_position(mut self, position: Point2<f32>) -> Self {
        self.position = position;
        self
    }

    #[inline]
    pub fn with_z_index(mut self, z_index: f32) -> Self {
        self.z_index = z_index;
        self
    }

    #[inline]
    pub fn with_rotation(mut self, rotation: Rotation2<f32>) -> Self {
        self.rotation = rotation;
        self
    }

    #[inline]
    pub fn with_scale(mut self, scale: Vector2<f32>) -> Self {
        self.scale = scale;
        self
    }

    #[inline]
    pub fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }

    #[inline]
    pub fn with_world(mut self, world: Similarity2<f32>) -> Self {
        self.world = world;
        self
    }

    #[inline]
    pub fn with_world_z_index(mut self, world_z_index: f32) -> Self {
        self.world_z_index = world_z_index;
        self
    }

    #[inline]
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }
}

impl CurveBuilder<Strong> {
    fn finalize(self, layer: &CanvasLayer) -> Curve {
        let id = Uuid::new_v4();
        let (layer_uuid, defaults, sender) = layer.parts();
        let hidden = self.hidden;
        let raw_curve = self.into_raw(defaults);

        if !hidden {
            sender.send(CurveEvent {
                id,
                layer: layer_uuid,
                kind: CurveEventKind::Created(Box::new(raw_curve.to_weak())),
            });
        }

        Curve::new(id, layer_uuid, raw_curve, hidden, sender.to_owned())
    }

    fn into_raw(self, defaults: &RenderDefaults) -> RawCurve<Strong> {
        RawCurve {
            pipeline: self
                .pipeline
                .unwrap_or_else(|| defaults.unlit_pipeline.clone()),
            path: self.path,
            stroke: self.stroke,
            position: self.position,
            z_index: self.z_index,
            rotation: self.rotation,
            scale: self.scale,
            tint: self.tint,
            world: self.world,
            world_z_index: self.world_z_index,
        }
    }
}

impl LayerSpawner for CurveBuilder<Strong> {
    type Handle = Curve;

    #[inline]
    fn spawn(self, layer: &CanvasLayer) -> Self::Handle {
        self.finalize(layer)
    }
}

impl<S> Default for CurveBuilder<S> {
    fn default() -> Self {
        Self {
            pipeline: None,
            path: Default::default(),
            stroke: Default::default(),
            position: Point2::origin(),
            z_index: 0.0,
            rotation: Rotation2::identity(),
            scale: super::vector2_one(),
            tint: super::arr4_one(),
            world: Similarity2::identity(),
            world_z_index: 0.0,
            hidden: false,
        }
    }
}

#[derive(Debug)]
pub struct Curve {
    id: Uuid,
    layer: Uuid,
    raw: RawCurve<Strong>,
    major_hash: u64,
    hidden: bool,
    sender: MessageSender,
}

impl Curve {
    fn new(
        id: Uuid,
        layer: Uuid,
        raw: RawCurve<Strong>,
        hidden: bool,
        sender: MessageSender,
    ) -> Self {
        let major_hash = raw.major_hash();

        Self {
            id,
            layer,
            raw,
            major_hash,
            hidden,
            sender,
        }
    }

    pub fn builder() -> CurveBuilder<Strong> {
        CurveBuilder::default()
    }

    #[inline]
    pub fn id(&self) -> Uuid {
        self.id
    }

    #[inline]
    pub fn modify(&mut self) -> CurveModify {
        CurveModify {
            new_hidden: self.hidden,
            underlying: self,
        }
    }
}

impl Deref for Curve {
    type Target = RawCurve<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl Clone for Curve {
    #[inline]
    fn clone(&self) -> Self {
        let id = Uuid::new_v4();

        if !self.hidden {
            self.sender.send(CurveEvent {
                id,
                layer: self.layer,
                kind: CurveEventKind::Created(Box::new(self.raw.to_weak())),
            });
        }

        Curve {
            id,
            layer: self.layer,
            raw: self.raw.clone(),
            major_hash: self.major_hash,
            hidden: self.hidden,
            sender: self.sender.clone(),
        }
    }
}

impl Drop for Curve {
    #[inline]
    fn drop(&mut self) {
        if !self.hidden {
            self.sender.send(CurveEvent {
                id: self.id,
                layer: self.layer,
                kind: CurveEventKind::Dropped,
            });
        }
    }
}

pub struct CurveModify<'a> {
    new_hidden: bool,
    underlying: &'a mut Curve,
}

impl<'a> CurveModify<'a> {
    pub fn hide(&mut self) {
        self.new_hidden = true;
    }

    pub fn show(&mut self) {
        self.new_hidden = false;
    }
}

impl<'a> Deref for CurveModify<'a> {
    type Target = RawCurve<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.underlying.raw
    }
}

impl<'a> DerefMut for CurveModify<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.underlying.raw
    }
}

impl<'a> Drop for CurveModify<'a> {
    #[inline]
    fn drop(&mut self) {
        let visibility_changed = self.underlying.hidden != self.new_hidden;
        self.underlying.hidden = self.new_hidden;

        let new_major_hash = self.underlying.raw.major_hash();
        let major_change = self.underlying.major_hash != new_major_hash;
        self.underlying.major_hash = new_major_hash;

        if visibility_changed && self.underlying.hidden {
            self.underlying.sender.send(CurveEvent {
                id: self.underlying.id,
                layer: self.underlying.layer,
                kind: CurveEventKind::Dropped,
            });
        } else if !self.underlying.hidden {
            self.underlying.sender.send(CurveEvent {
                id: self.underlying.id,
                layer: self.underlying.layer,
                kind: CurveEventKind::Modified {
                    raw: Box::new(self.underlying.raw.to_weak()),
                    major_change,
                },
            });
        }
    }
}
