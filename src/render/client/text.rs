use crate::asset::{AssetId, Strong};
use crate::prelude::MessageSender;
use crate::render::client::{CanvasLayer, LayerSpawner, RenderDefaults};
use crate::render::message::{TextEvent, TextEventKind};
use crate::render::pipeline::Pipeline;
use crate::render::text::{Font, HorizontalAlignment, RawText, VerticalAlignment};
use crate::util::{Bounded, Bounds};
use nalgebra::{Isometry2, Point3, Rotation2, Vector2};
use serde::Deserialize;
use std::borrow::Cow;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBuilder<S> {
    #[serde(default, bound(deserialize = "AssetId<Pipeline, S>: Deserialize<'de>"))]
    pub pipeline: Option<AssetId<Pipeline, S>>,
    #[serde(default, bound(deserialize = "AssetId<Font, S>: Deserialize<'de>"))]
    pub font: Option<AssetId<Font, S>>,
    #[serde(default)]
    pub content: Cow<'static, str>,
    #[serde(default = "Point3::origin")]
    pub position: Point3<f32>,
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
    #[serde(default = "super::arr3_one")]
    pub tint: [f32; 3],
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
    pub fn with_content<I: Into<Cow<'static, str>>>(mut self, content: I) -> Self {
        self.content = content.into();
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
    pub fn with_tint(mut self, tint: [f32; 3]) -> Self {
        self.tint = tint;
        self
    }
}

impl TextBuilder<Strong> {
    fn finalize(self, layer: &CanvasLayer) -> Text {
        let id = Uuid::new_v4();

        let raw_text = self.into_raw(layer.defaults());
        layer.sender.send(TextEvent {
            id,
            layer: layer.id(),
            kind: TextEventKind::Created(Box::new(raw_text.to_weak())),
        });

        Text::new(id, layer.id(), raw_text, layer.sender.clone())
    }

    fn into_raw(self, defaults: &RenderDefaults) -> RawText<Strong> {
        RawText {
            pipeline: self
                .pipeline
                .unwrap_or_else(|| defaults.text_pipeline.clone()),
            font: self.font.unwrap_or_else(|| defaults.font.clone()),
            content: self.content,
            position: self.position,
            rotation: self.rotation,
            point: self.point,
            width: self.width,
            height: self.height,
            line_height: self.line_height,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            scale: self.scale,
            tint: self.tint,
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
            content: Default::default(),
            position: Point3::origin(),
            rotation: Rotation2::identity(),
            point: Text::default_text_size(),
            width: None,
            height: None,
            line_height: super::f32_one(),
            vertical_alignment: Default::default(),
            horizontal_alignment: Default::default(),
            scale: super::f32_one(),
            tint: super::arr3_one(),
        }
    }
}

#[derive(Debug)]
pub struct Text {
    id: Uuid,
    layer: Uuid,
    raw: RawText<Strong>,
    major_hash: u64,
    sender: MessageSender,
}

impl Text {
    const DEFAULT_TEXT_SIZE: f32 = 1.0;

    fn new(id: Uuid, layer: Uuid, raw: RawText<Strong>, sender: MessageSender) -> Self {
        let major_hash = raw.major_hash();

        Self {
            id,
            layer,
            raw,
            major_hash,
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
        TextModify(self)
    }

    fn default_text_size() -> f32 {
        Self::DEFAULT_TEXT_SIZE
    }
}

impl Bounded for Text {
    #[inline]
    fn bounds(&self) -> Bounds {
        Bounds {
            size: Vector2::new(
                self.width.unwrap_or_default() * self.scale,
                self.height.unwrap_or_default() * self.scale,
            ),
            isometry: Isometry2::new(
                Vector2::new(self.position.x, self.position.y),
                self.rotation.angle(),
            ),
        }
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

        self.sender.send(TextEvent {
            id,
            layer: self.layer,
            kind: TextEventKind::Created(Box::new(self.raw.to_weak())),
        });

        Text {
            id,
            layer: self.layer,
            raw: self.raw.clone(),
            major_hash: self.major_hash,
            sender: self.sender.clone(),
        }
    }
}

impl Drop for Text {
    #[inline]
    fn drop(&mut self) {
        self.sender.send(TextEvent {
            id: self.id,
            layer: self.layer,
            kind: TextEventKind::Dropped,
        });
    }
}

pub struct TextModify<'a>(&'a mut Text);

impl<'a> Deref for TextModify<'a> {
    type Target = RawText<Strong>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0.raw
    }
}

impl<'a> DerefMut for TextModify<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.raw
    }
}

impl<'a> Drop for TextModify<'a> {
    #[inline]
    fn drop(&mut self) {
        let new_major_hash = self.0.raw.major_hash();
        let major_change = self.0.major_hash != new_major_hash;
        self.0.major_hash = new_major_hash;

        self.0.sender.send(TextEvent {
            id: self.0.id,
            layer: self.0.layer,
            kind: TextEventKind::Modified {
                raw: Box::new(self.0.raw.to_weak()),
                major_change,
            },
        });
    }
}
