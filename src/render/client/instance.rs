use crate::asset::{AssetId, Strong};
use crate::render::canvas::RawInstance;
use crate::render::client::{CanvasLayer, LayerSpawner, RenderDefaults};
use crate::render::mesh::Mesh;
use crate::render::message::{InstanceEvent, InstanceEventKind};
use crate::render::pipeline::Pipeline;
use crate::render::view::Texture;
use nalgebra::{Isometry3, Similarity3, Vector3};
use roundabout::prelude::MessageSender;
use serde::Deserialize;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceBuilder<S> {
    #[serde(default, bound(deserialize = "AssetId<Pipeline, S>: Deserialize<'de>"))]
    pub pipeline: Option<AssetId<Pipeline, S>>,
    #[serde(default, bound(deserialize = "AssetId<Mesh, S>: Deserialize<'de>"))]
    pub mesh: Option<AssetId<Mesh, S>>,
    #[serde(default, bound(deserialize = "AssetId<Texture, S>: Deserialize<'de>"))]
    pub texture: Option<AssetId<Texture, S>>,
    #[serde(default)]
    pub texture_layer: u32,
    #[serde(default = "Isometry3::identity")]
    pub model: Isometry3<f32>,
    #[serde(default = "super::vector3_one")]
    pub scale: Vector3<f32>,
    #[serde(default = "super::arr4_one")]
    pub tint: [f32; 4],
    #[serde(default = "Similarity3::identity")]
    pub world: Similarity3<f32>,
    #[serde(default)]
    pub hidden: bool,
}

impl<S> InstanceBuilder<S> {
    #[inline]
    pub fn with_pipeline(mut self, pipeline: AssetId<Pipeline, S>) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    #[inline]
    pub fn with_mesh(mut self, mesh: AssetId<Mesh, S>) -> Self {
        self.mesh = Some(mesh);
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
    pub fn with_model(mut self, model: Isometry3<f32>) -> Self {
        self.model = model;
        self
    }

    #[inline]
    pub fn with_scale(mut self, scale: Vector3<f32>) -> Self {
        self.scale = scale;
        self
    }

    #[inline]
    pub fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }

    #[inline]
    pub fn with_world(mut self, world: Similarity3<f32>) -> Self {
        self.world = world;
        self
    }

    #[inline]
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }
}

impl InstanceBuilder<Strong> {
    fn finalize(self, layer: &CanvasLayer) -> Instance {
        let id = Uuid::new_v4();
        let (layer_uuid, defaults, sender) = layer.parts();
        let hidden = self.hidden;
        let raw = self.into_raw(&defaults);

        if !hidden {
            sender.send(InstanceEvent {
                id,
                layer: layer_uuid,
                kind: InstanceEventKind::Created(Box::new(raw.to_weak())),
            });
        }

        Instance {
            id,
            layer: layer_uuid,
            raw,
            hidden,
            sender: sender.to_owned(),
        }
    }

    fn into_raw(self, defaults: &RenderDefaults) -> RawInstance<Strong> {
        RawInstance {
            pipeline: self
                .pipeline
                .unwrap_or_else(|| defaults.unlit_pipeline.clone()),
            mesh: self
                .mesh
                .unwrap_or_else(|| defaults.unit_square_mesh.clone()),
            texture: self
                .texture
                .unwrap_or_else(|| defaults.white_texture.clone()),
            texture_layer: 0,
            model: self.model,
            scale: self.scale,
            tint: self.tint,
            world: self.world,
        }
    }
}

impl LayerSpawner for InstanceBuilder<Strong> {
    type Handle = Instance;

    #[inline]
    fn spawn(self, layer: &CanvasLayer) -> Self::Handle {
        self.finalize(layer)
    }
}

impl<S> Default for InstanceBuilder<S> {
    fn default() -> Self {
        Self {
            pipeline: None,
            mesh: None,
            texture: None,
            texture_layer: 0,
            model: Isometry3::identity(),
            scale: super::vector3_one(),
            tint: super::arr4_one(),
            world: Similarity3::identity(),
            hidden: false,
        }
    }
}

#[derive(Debug)]
pub struct Instance {
    id: Uuid,
    layer: Uuid,
    raw: RawInstance<Strong>,
    hidden: bool,
    sender: MessageSender,
}

impl Instance {
    #[inline]
    pub fn builder() -> InstanceBuilder<Strong> {
        InstanceBuilder::default()
    }

    #[inline]
    pub fn id(&self) -> Uuid {
        self.id
    }

    #[inline]
    pub fn modify(&mut self) -> InstanceModify {
        InstanceModify {
            new_hidden: self.hidden,
            underlying: self,
        }
    }
}

impl Deref for Instance {
    type Target = RawInstance<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl Clone for Instance {
    #[inline]
    fn clone(&self) -> Self {
        let id = Uuid::new_v4();

        if !self.hidden {
            self.sender.send(InstanceEvent {
                id,
                layer: self.layer,
                kind: InstanceEventKind::Created(Box::new(self.raw.to_weak())),
            });
        }

        Instance {
            id,
            layer: self.layer,
            raw: self.raw.clone(),
            hidden: self.hidden,
            sender: self.sender.clone(),
        }
    }
}

impl Drop for Instance {
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
pub struct InstanceModify<'a> {
    new_hidden: bool,
    underlying: &'a mut Instance,
}

impl<'a> InstanceModify<'a> {
    pub fn hide(&mut self) {
        self.new_hidden = true;
    }

    pub fn show(&mut self) {
        self.new_hidden = false;
    }
}

impl<'a> Deref for InstanceModify<'a> {
    type Target = RawInstance<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.underlying.raw
    }
}

impl<'a> DerefMut for InstanceModify<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.underlying.raw
    }
}

impl<'a> Drop for InstanceModify<'a> {
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
            self.underlying.sender.send(InstanceEvent {
                id: self.underlying.id,
                layer: self.underlying.layer,
                kind: InstanceEventKind::Modified(Box::new(self.underlying.raw.to_weak())),
            });
        }
    }
}
