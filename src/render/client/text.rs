use crate::asset::{AssetId, Strong};
use crate::prelude::MessageSender;
use crate::render::client::{CanvasLayer, LayerSpawner, RenderDefaults};
use crate::render::message::{TextEvent, TextEventKind};
use crate::render::pipeline::Pipeline;
use crate::render::text::{Font, HorizontalAlignment, RawText, VerticalAlignment};
use crate::util::{Bounded, Bounds};
use nalgebra::{Isometry2, Point2, Rotation2, Similarity2, Vector2};
use serde::Deserialize;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use uuid::Uuid;

fn arcstr_default() -> Arc<str> {
    "".to_string().into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBuilder<S> {
    #[serde(default, bound(deserialize = "AssetId<Pipeline, S>: Deserialize<'de>"))]
    pub pipeline: Option<AssetId<Pipeline, S>>,
    #[serde(default, bound(deserialize = "AssetId<Font, S>: Deserialize<'de>"))]
    pub font: Option<AssetId<Font, S>>,
    #[serde(default = "arcstr_default")]
    pub content: Arc<str>,
    #[serde(default = "Point2::origin")]
    pub position: Point2<f32>,
    #[serde(default)]
    pub z_index: f32,
    #[serde(default = "Rotation2::identity")]
    pub rotation: Rotation2<f32>,
    #[serde(default = "Text::default_text_size")]
    pub point: f32,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default = "super::f32_one")]
    pub line_height: f32,
    #[serde(default)]
    pub vertical_alignment: VerticalAlignment,
    #[serde(default)]
    pub horizontal_alignment: HorizontalAlignment,
    #[serde(default = "super::f32_one")]
    pub scale: f32,
    #[serde(default = "super::arr4_one")]
    pub tint: [f32; 4],
    #[serde(default = "Similarity2::identity")]
    pub world: Similarity2<f32>,
    #[serde(default)]
    pub world_z_index: f32,
    #[serde(default)]
    pub hidden: bool,
}

impl<S> TextBuilder<S> {
    #[inline]
    pub fn with_pipeline(mut self, pipeline: AssetId<Pipeline, S>) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    #[inline]
    pub fn with_font(mut self, font: AssetId<Font, S>) -> Self {
        self.font = Some(font);
        self
    }

    #[inline]
    pub fn with_content<I: Into<Arc<str>>>(mut self, content: I) -> Self {
        self.content = content.into();
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
    pub fn with_point(mut self, point: f32) -> Self {
        self.point = point;
        self
    }

    #[inline]
    pub fn with_width<I: Into<Option<f32>>>(mut self, width: I) -> Self {
        self.width = width.into();
        self
    }

    #[inline]
    pub fn with_height<I: Into<Option<f32>>>(mut self, height: I) -> Self {
        self.height = height.into();
        self
    }

    #[inline]
    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    #[inline]
    pub fn with_vertical_alignment(mut self, vertical_alignment: VerticalAlignment) -> Self {
        self.vertical_alignment = vertical_alignment;
        self
    }

    #[inline]
    pub fn with_horizontal_alignment(mut self, horizontal_alignment: HorizontalAlignment) -> Self {
        self.horizontal_alignment = horizontal_alignment;
        self
    }

    #[inline]
    pub fn with_scale(mut self, scale: f32) -> Self {
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

impl TextBuilder<Strong> {
    fn finalize(self, layer: &CanvasLayer) -> Text {
        let id = Uuid::new_v4();
        let (layer_uuid, defaults, sender) = layer.parts();
        let hidden = self.hidden;
        let raw_text = self.into_raw(defaults);

        if !hidden {
            sender.send(TextEvent {
                id,
                layer: layer_uuid,
                kind: TextEventKind::Created(Box::new(raw_text.to_weak())),
            });
        }

        Text::new(id, layer_uuid, raw_text, hidden, sender.clone())
    }

    fn into_raw(self, defaults: &RenderDefaults) -> RawText<Strong> {
        RawText {
            pipeline: self
                .pipeline
                .unwrap_or_else(|| defaults.text_pipeline.clone()),
            font: self.font.unwrap_or_else(|| defaults.font.clone()),
            content: self.content,
            position: self.position,
            z_index: self.z_index,
            rotation: self.rotation,
            point: self.point,
            width: self.width,
            height: self.height,
            line_height: self.line_height,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            scale: self.scale,
            tint: self.tint,
            world: self.world,
            world_z_index: self.world_z_index,
        }
    }
}

impl LayerSpawner for TextBuilder<Strong> {
    type Handle = Text;

    #[inline]
    fn spawn(self, layer: &CanvasLayer) -> Self::Handle {
        self.finalize(layer)
    }
}

impl<S> Default for TextBuilder<S> {
    fn default() -> Self {
        Self {
            pipeline: None,
            font: None,
            content: arcstr_default(),
            position: Point2::origin(),
            z_index: 0.0,
            rotation: Rotation2::identity(),
            point: Text::default_text_size(),
            width: None,
            height: None,
            line_height: super::f32_one(),
            vertical_alignment: Default::default(),
            horizontal_alignment: Default::default(),
            scale: super::f32_one(),
            tint: super::arr4_one(),
            world: Similarity2::identity(),
            world_z_index: 0.0,
            hidden: false,
        }
    }
}

#[derive(Debug)]
pub struct Text {
    id: Uuid,
    layer: Uuid,
    raw: RawText<Strong>,
    major_hash: u64,
    hidden: bool,
    sender: MessageSender,
}

impl Text {
    const DEFAULT_TEXT_SIZE: f32 = 1.0;

    fn new(
        id: Uuid,
        layer: Uuid,
        raw: RawText<Strong>,
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

    #[inline]
    pub fn builder() -> TextBuilder<Strong> {
        TextBuilder::default()
    }

    #[inline]
    pub fn id(&self) -> Uuid {
        self.id
    }

    #[inline]
    pub fn modify(&mut self) -> TextModify {
        TextModify {
            new_hidden: self.hidden,
            underlying: self,
        }
    }

    fn default_text_size() -> f32 {
        Self::DEFAULT_TEXT_SIZE
    }
}

impl Bounded for Text {
    #[inline]
    fn bounds(&self) -> Bounds {
        let extends = Vector2::new(
            self.width.unwrap_or_default() * self.scale,
            self.height.unwrap_or_default() * self.scale,
        );
        let half_extends = extends / 2.0;
        let model = self.world * Isometry2::new(self.position.coords, self.rotation.angle());

        let o = model * Point2::from(half_extends);
        let w = model * Point2::new(half_extends.x, -half_extends.y);
        let h = model * Point2::new(-half_extends.x, half_extends.y);

        Bounds { o, w, h }
    }
}

impl Deref for Text {
    type Target = RawText<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl Clone for Text {
    #[inline]
    fn clone(&self) -> Self {
        let id = Uuid::new_v4();

        if !self.hidden {
            self.sender.send(TextEvent {
                id,
                layer: self.layer,
                kind: TextEventKind::Created(Box::new(self.raw.to_weak())),
            });
        }

        Text {
            id,
            layer: self.layer,
            raw: self.raw.clone(),
            major_hash: self.major_hash,
            hidden: self.hidden,
            sender: self.sender.clone(),
        }
    }
}

impl Drop for Text {
    #[inline]
    fn drop(&mut self) {
        if !self.hidden {
            self.sender.send(TextEvent {
                id: self.id,
                layer: self.layer,
                kind: TextEventKind::Dropped,
            });
        }
    }
}

pub struct TextModify<'a> {
    new_hidden: bool,
    underlying: &'a mut Text,
}

impl<'a> TextModify<'a> {
    pub fn hide(&mut self) {
        self.new_hidden = true;
    }

    pub fn show(&mut self) {
        self.new_hidden = false;
    }
}

impl<'a> Deref for TextModify<'a> {
    type Target = RawText<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.underlying.raw
    }
}

impl<'a> DerefMut for TextModify<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.underlying.raw
    }
}

impl<'a> Drop for TextModify<'a> {
    #[inline]
    fn drop(&mut self) {
        let visibility_changed = self.underlying.hidden != self.new_hidden;
        self.underlying.hidden = self.new_hidden;

        let new_major_hash = self.underlying.raw.major_hash();
        let major_change = self.underlying.major_hash != new_major_hash;
        self.underlying.major_hash = new_major_hash;

        if visibility_changed && self.underlying.hidden {
            self.underlying.sender.send(TextEvent {
                id: self.underlying.id,
                layer: self.underlying.layer,
                kind: TextEventKind::Dropped,
            });
        } else if !self.underlying.hidden {
            self.underlying.sender.send(TextEvent {
                id: self.underlying.id,
                layer: self.underlying.layer,
                kind: TextEventKind::Modified {
                    raw: Box::new(self.underlying.raw.to_weak()),
                    major_change,
                },
            });
        }
    }
}
