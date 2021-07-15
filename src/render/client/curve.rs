use crate::asset::{AssetId, Strong};
use crate::render::client::{CanvasLayer, LayerSpawner, RenderDefaults};
use crate::render::curve::{Path, RawCurve, StrokeOptions};
use crate::render::message::{CurveEvent, CurveEventKind};
use crate::render::pipeline::Pipeline;
use nalgebra::{Point3, Rotation2, Vector2};
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
    #[serde(default = "Point3::origin")]
    pub position: Point3<f32>,
    #[serde(default = "Rotation2::identity")]
    pub rotation: Rotation2<f32>,
    #[serde(default = "super::vector2_one")]
    pub scale: Vector2<f32>,
    #[serde(default = "super::arr3_one")]
    pub tint: [f32; 3],
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
    pub fn with_position(mut self, position: Point3<f32>) -> Self {
        self.position = position;
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
    pub fn with_tint(mut self, tint: [f32; 3]) -> Self {
        self.tint = tint;
        self
    }
}

impl CurveBuilder<Strong> {
    fn finalize(self, layer: &CanvasLayer) -> Curve {
        let id = Uuid::new_v4();

        let raw_curve = self.into_raw(layer.defaults());
        layer.sender.send(CurveEvent {
            id,
            layer: layer.id(),
            kind: CurveEventKind::Created(Box::new(raw_curve.to_weak())),
        });

        Curve::new(id, layer.id(), raw_curve, layer.sender.clone())
    }

    fn into_raw(self, defaults: &RenderDefaults) -> RawCurve<Strong> {
        RawCurve {
            pipeline: self
                .pipeline
                .unwrap_or_else(|| defaults.unlit_pipeline.clone()),
            path: self.path,
            stroke: self.stroke,
            position: self.position,
            rotation: self.rotation,
            scale: self.scale,
            tint: self.tint,
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
            position: Point3::origin(),
            rotation: Rotation2::identity(),
            scale: super::vector2_one(),
            tint: super::arr3_one(),
        }
    }
}

#[derive(Debug)]
pub struct Curve {
    id: Uuid,
    layer: Uuid,
    raw: RawCurve<Strong>,
    major_hash: u64,
    sender: MessageSender,
}

impl Curve {
    fn new(id: Uuid, layer: Uuid, raw: RawCurve<Strong>, sender: MessageSender) -> Self {
        let major_hash = raw.major_hash();

        Self {
            id,
            layer,
            raw,
            major_hash,
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
        CurveModify(self)
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

        self.sender.send(CurveEvent {
            id,
            layer: self.layer,
            kind: CurveEventKind::Created(Box::new(self.raw.to_weak())),
        });

        Curve {
            id,
            layer: self.layer,
            raw: self.raw.clone(),
            major_hash: self.major_hash,
            sender: self.sender.clone(),
        }
    }
}

impl Drop for Curve {
    #[inline]
    fn drop(&mut self) {
        self.sender.send(CurveEvent {
            id: self.id,
            layer: self.layer,
            kind: CurveEventKind::Dropped,
        });
    }
}

pub struct CurveModify<'a>(&'a mut Curve);

impl<'a> Deref for CurveModify<'a> {
    type Target = RawCurve<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0.raw
    }
}

impl<'a> DerefMut for CurveModify<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.raw
    }
}

impl<'a> Drop for CurveModify<'a> {
    #[inline]
    fn drop(&mut self) {
        let new_major_hash = self.0.raw.major_hash();
        let major_change = self.0.major_hash != new_major_hash;
        self.0.major_hash = new_major_hash;

        self.0.sender.send(CurveEvent {
            id: self.0.id,
            layer: self.0.layer,
            kind: CurveEventKind::Modified {
                raw: Box::new(self.0.raw.to_weak()),
                major_change,
            },
        });
    }
}
