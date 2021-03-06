use crate::asset::{AssetId, Strong, StrongAssetId, Weak};
use crate::render::canvas::RawInstance;
use crate::render::client::{CanvasLayer, LayerSpawner, RenderDefaults};
use crate::render::mesh::Mesh;
use crate::render::message::{InstanceEvent, InstanceEventKind};
use crate::render::pipeline::Pipeline;
use crate::render::view::Texture;
use crate::util::{Bounded, Bounds};
use nalgebra::{
    Isometry2, Isometry3, Point2, Rotation2, Similarity2, Similarity3, Translation3,
    UnitQuaternion, Vector2, Vector3,
};
use roundabout::prelude::MessageSender;
use serde::Deserialize;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

#[derive(Debug, Copy, Clone)]
pub struct RawSprite<S> {
    pub pipeline: AssetId<Pipeline, S>,
    pub texture: AssetId<Texture, S>,
    pub texture_layer: u32,
    pub position: Point2<f32>,
    pub z_index: f32,
    pub rotation: Rotation2<f32>,
    pub size: Vector2<f32>,
    pub scale: Vector2<f32>,
    pub tint: [f32; 4],
    pub world: Similarity2<f32>,
    pub world_z_index: f32,
}

impl<S> RawSprite<S> {
    fn to_weak(&self) -> RawSprite<Weak> {
        RawSprite {
            pipeline: self.pipeline.to_weak(),
            texture: self.texture.to_weak(),
            texture_layer: self.texture_layer,
            position: self.position,
            z_index: self.z_index,
            rotation: self.rotation,
            size: self.size,
            scale: self.scale,
            tint: self.tint,
            world: self.world,
            world_z_index: self.world_z_index,
        }
    }

    fn into_raw_instance(self, unit_square_mesh: AssetId<Mesh, S>) -> RawInstance<S> {
        RawInstance {
            pipeline: self.pipeline,
            mesh: unit_square_mesh,
            texture: self.texture,
            texture_layer: self.texture_layer,
            model: Isometry3::from_parts(
                Translation3::new(self.position.x, self.position.y, self.z_index),
                UnitQuaternion::from_axis_angle(&Vector3::z_axis(), self.rotation.angle()),
            ),
            scale: Vector3::new(self.size.x * self.scale.x, self.size.y * self.scale.y, 1.0),
            tint: self.tint,
            world: Similarity3::from_parts(
                Translation3::new(
                    self.world.isometry.translation.x,
                    self.world.isometry.translation.y,
                    self.world_z_index,
                ),
                UnitQuaternion::from_axis_angle(
                    &Vector3::z_axis(),
                    self.world.isometry.rotation.angle(),
                ),
                self.world.scaling(),
            ),
        }
    }
}

#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteBuilder<S> {
    #[serde(default, bound(deserialize = "AssetId<Pipeline, S>: Deserialize<'de>"))]
    pub pipeline: Option<AssetId<Pipeline, S>>,
    #[serde(default, bound(deserialize = "AssetId<Texture, S>: Deserialize<'de>"))]
    pub texture: Option<AssetId<Texture, S>>,
    #[serde(default)]
    pub texture_layer: u32,
    #[serde(default = "Point2::origin")]
    pub position: Point2<f32>,
    #[serde(default)]
    pub z_index: f32,
    #[serde(default = "Rotation2::identity")]
    pub rotation: Rotation2<f32>,
    #[serde(default = "super::vector2_one")]
    pub size: Vector2<f32>,
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

impl<S> SpriteBuilder<S> {
    #[inline]
    pub fn with_pipeline(mut self, pipeline: AssetId<Pipeline, S>) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    #[inline]
    pub fn with_texture(mut self, texture: AssetId<Texture, S>) -> Self {
        self.texture = Some(texture);
        self
    }

    #[inline]
    pub fn with_texture_layer(mut self, texture_layer: u32) -> Self {
        self.texture_layer = texture_layer;
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
    pub fn with_size(mut self, size: Vector2<f32>) -> Self {
        self.size = size;
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

impl SpriteBuilder<Strong> {
    fn finalize(self, layer: &CanvasLayer) -> Sprite {
        let id = Uuid::new_v4();
        let (layer_uuid, defaults, sender) = layer.parts();
        let hidden = self.hidden;
        let unit_square_mesh = defaults.unit_square_mesh.clone();
        let raw_rectangle = self.into_raw(defaults);

        if !hidden {
            let raw_instance = raw_rectangle
                .to_weak()
                .into_raw_instance(unit_square_mesh.to_weak());
            sender.send(InstanceEvent {
                id,
                layer: layer_uuid,
                kind: InstanceEventKind::Created(Box::new(raw_instance)),
            });
        }

        Sprite {
            id,
            layer: layer_uuid,
            unit_square_mesh,
            raw: raw_rectangle,
            hidden,
            sender: sender.to_owned(),
        }
    }

    fn into_raw(self, defaults: &RenderDefaults) -> RawSprite<Strong> {
        RawSprite {
            pipeline: self
                .pipeline
                .unwrap_or_else(|| defaults.unlit_pipeline.clone()),
            texture: self
                .texture
                .unwrap_or_else(|| defaults.white_texture.clone()),
            texture_layer: self.texture_layer,
            position: self.position,
            z_index: self.z_index,
            rotation: self.rotation,
            size: self.size,
            scale: self.scale,
            tint: self.tint,
            world: self.world,
            world_z_index: self.world_z_index,
        }
    }
}

impl LayerSpawner for SpriteBuilder<Strong> {
    type Handle = Sprite;

    #[inline]
    fn spawn(self, layer: &CanvasLayer) -> Self::Handle {
        self.finalize(layer)
    }
}

impl<S> Default for SpriteBuilder<S> {
    fn default() -> Self {
        Self {
            pipeline: None,
            texture: None,
            texture_layer: 0,
            position: Point2::origin(),
            z_index: 0.0,
            rotation: Rotation2::identity(),
            size: super::vector2_one(),
            scale: super::vector2_one(),
            tint: super::arr4_one(),
            world: Similarity2::identity(),
            world_z_index: 0.0,
            hidden: false,
        }
    }
}

#[derive(Debug)]
pub struct Sprite {
    id: Uuid,
    layer: Uuid,
    unit_square_mesh: StrongAssetId<Mesh>,
    raw: RawSprite<Strong>,
    hidden: bool,
    sender: MessageSender,
}

impl Sprite {
    #[inline]
    pub fn builder() -> SpriteBuilder<Strong> {
        SpriteBuilder::default()
    }

    #[inline]
    pub fn id(&self) -> Uuid {
        self.id
    }

    #[inline]
    pub fn modify(&mut self) -> SpriteModify {
        SpriteModify {
            new_hidden: self.hidden,
            underlying: self,
        }
    }
}

impl Bounded for Sprite {
    #[inline]
    fn bounds(&self) -> Bounds {
        let extends = Vector2::new(self.size.x * self.scale.x, self.size.y * self.scale.y);
        let half_extends = extends / 2.0;
        let model = self.world * Isometry2::new(self.position.coords, self.rotation.angle());

        let o = model * Point2::from(half_extends);
        let w = model * Point2::new(half_extends.x, -half_extends.y);
        let h = model * Point2::new(-half_extends.x, half_extends.y);

        Bounds { o, w, h }
    }
}

impl Deref for Sprite {
    type Target = RawSprite<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl Clone for Sprite {
    #[inline]
    fn clone(&self) -> Self {
        let id = Uuid::new_v4();

        if !self.hidden {
            let raw_instance = self
                .raw
                .to_weak()
                .into_raw_instance(self.unit_square_mesh.to_weak());
            self.sender.send(InstanceEvent {
                id,
                layer: self.layer,
                kind: InstanceEventKind::Created(Box::new(raw_instance)),
            });
        }

        Sprite {
            id,
            layer: self.layer,
            unit_square_mesh: self.unit_square_mesh.clone(),
            raw: self.raw.clone(),
            hidden: self.hidden,
            sender: self.sender.clone(),
        }
    }
}

impl Drop for Sprite {
    #[inline]
    fn drop(&mut self) {
        if !self.hidden {
            self.sender.send(InstanceEvent {
                id: self.id,
                layer: self.layer,
                kind: InstanceEventKind::Dropped,
            });
        }
    }
}

#[derive(Debug)]
pub struct SpriteModify<'a> {
    new_hidden: bool,
    underlying: &'a mut Sprite,
}

impl<'a> SpriteModify<'a> {
    pub fn hide(&mut self) {
        self.new_hidden = true;
    }

    pub fn show(&mut self) {
        self.new_hidden = false;
    }
}

impl<'a> Deref for SpriteModify<'a> {
    type Target = RawSprite<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.underlying.raw
    }
}

impl<'a> DerefMut for SpriteModify<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.underlying.raw
    }
}

impl<'a> Drop for SpriteModify<'a> {
    #[inline]
    fn drop(&mut self) {
        let visibility_changed = self.underlying.hidden != self.new_hidden;
        self.underlying.hidden = self.new_hidden;

        if visibility_changed && self.underlying.hidden {
            self.underlying.sender.send(InstanceEvent {
                id: self.underlying.id,
                layer: self.underlying.layer,
                kind: InstanceEventKind::Dropped,
            });
        } else if !self.underlying.hidden {
            let raw_instance = self
                .underlying
                .raw
                .to_weak()
                .into_raw_instance(self.underlying.unit_square_mesh.to_weak());
            self.underlying.sender.send(InstanceEvent {
                id: self.underlying.id,
                layer: self.underlying.layer,
                kind: InstanceEventKind::Modified(Box::new(raw_instance)),
            });
        }
    }
}
