use crate::asset::storage::AssetsClient;
use crate::asset::{AssetId, StrongAssetId, Weak, WeakAssetId};
use crate::prelude::Texture;
use crate::render::buffer::Vertex;
use crate::render::canvas::RawInstance;
use crate::render::mesh::Mesh;
use crate::render::pipeline::Pipeline;
use crate::render::view::FilterMode;
use crate::util::{HashMap, OrderWindow};
use ahash::AHasher;
use copyless::VecHelper;
use nalgebra::{
    Isometry3, Point2, Rotation2, Similarity2, Similarity3, Translation3, UnitQuaternion, Vector3,
};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::hash::{Hash, Hasher};
use unicode_linebreak::BreakOpportunity;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Font {
    pub texture: StrongAssetId<Texture>,
    pub layout: StrongAssetId<FontLayout>,
}

impl Font {
    #[rustfmt::skip]
    const FONT_IMAGE_UUID: Uuid = Uuid::from_bytes([
        0x25, 0x7a, 0xee, 0x64, 0x55, 0x40, 0x4d, 0x35,
        0xa3, 0xb7, 0x29, 0xd0, 0x09, 0x11, 0x72, 0xac
    ]);

    #[rustfmt::skip]
    const FONT_TEXTURE_UUID: Uuid = Uuid::from_bytes([
        0x48, 0x77, 0xa7, 0x53, 0x28, 0xb1, 0x4a, 0x97,
        0x85, 0xff, 0x53, 0x18, 0xbd, 0xf7, 0xde, 0x5f
    ]);

    #[rustfmt::skip]
    const FONT_LAYOUT_UUID: Uuid = Uuid::from_bytes([
        0x30, 0x11, 0x18, 0x0b, 0x31, 0x01, 0x49, 0xf4,
        0xa0, 0x09, 0x70, 0xc1, 0xf6, 0x63, 0x0c, 0x17
    ]);

    #[rustfmt::skip]
    const FONT_UUID: Uuid = Uuid::from_bytes([
        0xf8, 0x8c, 0x63, 0x90, 0x75, 0x39, 0x4a, 0x65,
        0x93, 0xfb, 0x49, 0xdc, 0xc6, 0x50, 0x0c, 0xc1
    ]);
}

#[derive(Debug, Copy, Clone)]
struct Atom {
    glyph: Glyph,
    whitespace: bool,

    line: usize,
    line_until_advance: f32,
    scaled_advance: f32,
    is_advance: bool,

    do_break: bool,
    mandatory_break: bool,
    allowed_break: bool,

    line_until_advance_count: usize,
    line_until_allowed_break_count: usize,
}

pub struct LinebreakIter<T: Clone + Iterator<Item = (usize, BreakOpportunity)>> {
    underlying: T,
    mandatory: usize,
    allowed: usize,
}

impl<T: Clone + Iterator<Item = (usize, BreakOpportunity)>> LinebreakIter<T> {
    pub fn new(underlying: T) -> Self {
        let mut s = Self {
            underlying,
            mandatory: usize::MAX,
            allowed: usize::MAX,
        };

        s.advance();

        s
    }

    pub fn advance(&mut self) {
        match self.underlying.next() {
            Some((u, BreakOpportunity::Mandatory)) => {
                self.mandatory = u;
                self.allowed = usize::MAX;
            }
            Some((u, BreakOpportunity::Allowed)) => {
                self.mandatory = usize::MAX;
                self.allowed = u;
            }
            None => {
                self.mandatory = usize::MAX;
                self.allowed = usize::MAX;
                return;
            }
        };
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "FontLayoutData")]
pub struct FontLayout {
    pub glyphs: HashMap<char, Glyph>,
    pub line_height: f32,
    pub size: f32,
    pub distance_range: f32,
}

impl FontLayout {
    pub(crate) fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let layout_data = serde_json::from_slice(bytes)?;
        Self::from_data(layout_data)
    }

    fn from_data(data: FontLayoutData) -> anyhow::Result<Self> {
        let line_height = data.metrics.line_height;

        let mut glyphs: HashMap<char, Glyph> = Default::default();

        let height = data.atlas.height;
        let epsilon_x_half = 1.0 / (data.atlas.width * 2.0);
        let epsilon_y_half = 1.0 / (data.atlas.height * 2.0);

        for l_glyph in &data.glyphs {
            let unicode = std::char::from_u32(l_glyph.unicode).ok_or_else(|| {
                anyhow::anyhow!("Invalid unicode scalar value: {}", l_glyph.unicode)
            })?;

            let vertices = if let (Some(plane_bounds), Some(atlas_bounds)) =
                (&l_glyph.plane_bounds, &l_glyph.atlas_bounds)
            {
                Some([
                    // top-left
                    GlyphVertex {
                        position: Point2::new(plane_bounds.left, plane_bounds.top),
                        tex_coords: [
                            atlas_bounds.left / data.atlas.width + epsilon_x_half,
                            (height - atlas_bounds.top) / data.atlas.height + epsilon_y_half,
                        ],
                    },
                    // top-right
                    GlyphVertex {
                        position: Point2::new(plane_bounds.right, plane_bounds.top),
                        tex_coords: [
                            atlas_bounds.right / data.atlas.width - epsilon_x_half,
                            (height - atlas_bounds.top) / data.atlas.height + epsilon_y_half,
                        ],
                    },
                    // bottom-right
                    GlyphVertex {
                        position: Point2::new(plane_bounds.right, plane_bounds.bottom),
                        tex_coords: [
                            atlas_bounds.right / data.atlas.width - epsilon_x_half,
                            (height - atlas_bounds.bottom) / data.atlas.height - epsilon_y_half,
                        ],
                    },
                    // bottom-left
                    GlyphVertex {
                        position: Point2::new(plane_bounds.left, plane_bounds.bottom),
                        tex_coords: [
                            atlas_bounds.left / data.atlas.width + epsilon_x_half,
                            (height - atlas_bounds.bottom) / data.atlas.height - epsilon_y_half,
                        ],
                    },
                ])
            } else {
                None
            };

            glyphs.insert(
                unicode,
                Glyph {
                    advance: l_glyph.advance,
                    vertices,
                },
            );
        }

        Ok(Self {
            glyphs,
            line_height,
            size: data.atlas.size,
            distance_range: data.atlas.distance_range,
        })
    }

    #[inline]
    pub fn generate_mesh<S>(&self, text: &RawText<S>) -> anyhow::Result<Mesh> {
        let mut linebreaker = LinebreakIter::new(unicode_linebreak::linebreaks(&text.content));
        let mut atoms: Vec<Atom> = Vec::with_capacity(text.content.len());

        for (glyph_i, c) in text.content.char_indices() {
            let glyph = if c.is_control() {
                &Glyph::NOOP
            } else {
                self.glyphs.get(&c).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Font is missing glyph for: '{}' = {}",
                        c,
                        c.escape_unicode()
                    )
                })?
            };

            let scaled_advance = glyph.advance * text.point;
            let mandatory_break = linebreaker.mandatory == glyph_i;
            let allowed_break = linebreaker.allowed == glyph_i;

            if mandatory_break || allowed_break {
                linebreaker.advance();
            }

            atoms.push(Atom {
                glyph: *glyph,
                whitespace: c.is_whitespace(),

                line: 0,
                line_until_advance: 0.0,
                scaled_advance,
                is_advance: glyph.advance > 0.01,

                do_break: mandatory_break,
                mandatory_break,
                allowed_break,

                line_until_advance_count: 0,
                line_until_allowed_break_count: 0,
            });
        }

        let mut linebreaks: Vec<usize> = Vec::default();
        let mut atom_index = 0;
        let mut last_possible_break = 0;
        loop {
            if atom_index >= atoms.len() {
                break;
            }

            let (
                line,
                line_until_advance,
                line_until_advance_count,
                line_until_allowed_break_count,
            ) = if atom_index == 0 {
                (0, 0.0, 0, 0)
            } else {
                atoms
                    .get(atom_index - 1)
                    .map(|a| {
                        (
                            a.line,
                            a.line_until_advance + a.scaled_advance,
                            a.line_until_advance_count,
                            a.line_until_allowed_break_count,
                        )
                    })
                    .unwrap()
            };

            let is_x_overflow = text.width.unwrap_or(f32::INFINITY) < line_until_advance as f32;
            if is_x_overflow
                && last_possible_break > linebreaks.last().copied().unwrap_or_default()
                && !atoms.get_mut(atom_index).unwrap().do_break
            {
                // x overflow with breakpoint available
                let last_possible = atoms.get_mut(last_possible_break).unwrap();
                last_possible.do_break = true;
                atom_index = last_possible_break;
            } else {
                // x within bounds or no breakpoint available
                let current = &mut atoms[atom_index];

                if current.mandatory_break || current.allowed_break {
                    last_possible_break = atom_index;
                }

                if current.do_break || atom_index == 0 {
                    current.is_advance = false;
                    if atom_index > 0 {
                        linebreaks.entry(line).set(atom_index);
                        current.line = line + 1;
                    }
                    current.line_until_advance = 0.0;
                    current.line_until_advance_count = 0;
                    current.line_until_allowed_break_count = 0;
                } else {
                    current.is_advance = current.glyph.advance > 0.01;
                    current.line = line;
                    current.line_until_advance = line_until_advance;
                    current.line_until_advance_count =
                        line_until_advance_count + current.is_advance as usize;
                    current.line_until_allowed_break_count =
                        line_until_allowed_break_count + current.allowed_break as usize;
                }

                atom_index += 1;
            }
        }

        let mut vertices = Vec::default();
        let mut indices = Vec::default();
        let translation_y = match text.vertical_alignment {
            VerticalAlignment::Top => {
                let rect_height = text.height.unwrap_or_default();
                -self.line_height * text.line_height * text.point + rect_height / 2.0
            }
            VerticalAlignment::Center => {
                let text_height = (linebreaks.len() as f32 - 0.5)
                    * self.line_height
                    * text.line_height
                    * text.point;
                text_height / 2.0
            }
            VerticalAlignment::Bottom => {
                let rect_height = text.height.unwrap_or_default();
                let text_height =
                    linebreaks.len() as f32 * self.line_height * text.line_height * text.point;

                -rect_height / 2.0 + text_height
            }
        };
        let mut vertex_index = 0;

        let mut current_line = usize::MAX;
        let mut translation_x = 0.0;
        let mut non_break_advance_offset = 0.0;
        let mut break_advance_offset = 0.0;

        for atom in &atoms {
            if current_line != atom.line {
                current_line = atom.line;
                // last line has not a linebreak
                let prev_break_atom = if let Some(i) = linebreaks.get(current_line) {
                    &atoms[i - 1]
                } else {
                    atoms.last().unwrap()
                };

                match text.horizontal_alignment {
                    HorizontalAlignment::Left => {
                        translation_x = -text.width.unwrap_or_default() / 2.0;
                    }
                    HorizontalAlignment::Right => {
                        let rect_width = text.width.unwrap_or_default();
                        let line_advance = if prev_break_atom.whitespace {
                            prev_break_atom.line_until_advance
                        } else {
                            prev_break_atom.line_until_advance + prev_break_atom.scaled_advance
                        };
                        translation_x =
                            text.width.unwrap_or_default() - line_advance - rect_width / 2.0;
                    }
                    HorizontalAlignment::Center => {
                        let rect_width = text.width.unwrap_or_default();
                        let line_advance = if prev_break_atom.whitespace {
                            prev_break_atom.line_until_advance
                        } else {
                            prev_break_atom.line_until_advance + prev_break_atom.scaled_advance
                        };
                        translation_x =
                            (text.width.unwrap_or_default() - line_advance - rect_width) / 2.0;
                    }
                    HorizontalAlignment::Justified => {
                        let mut non_break_count = prev_break_atom.line_until_advance_count
                            - prev_break_atom.line_until_allowed_break_count
                            - 1;
                        let break_aspect = prev_break_atom.line_until_allowed_break_count as f32
                            / prev_break_atom.line_until_advance_count as f32;

                        let mut line_advance = prev_break_atom.line_until_advance;
                        if !prev_break_atom.whitespace {
                            line_advance += prev_break_atom.scaled_advance;
                            non_break_count += 1;
                        }
                        let negative_advance = text.width.unwrap_or_default() - line_advance;

                        translation_x = -text.width.unwrap_or_default() / 2.0;

                        break_advance_offset = (negative_advance
                            / prev_break_atom.line_until_allowed_break_count as f32)
                            .max(0.0)
                            .min(negative_advance * break_aspect * 5.0);

                        let remaining_negative_advance = negative_advance
                            - break_advance_offset
                                * prev_break_atom.line_until_allowed_break_count as f32;
                        non_break_advance_offset =
                            (remaining_negative_advance / non_break_count as f32).max(0.0);
                    }
                };
            }

            translation_x += if atom.allowed_break && !atom.do_break {
                break_advance_offset
            } else if atom.is_advance {
                non_break_advance_offset
            } else {
                0.0
            };

            if let Some(g_vertices) = &atom.glyph.vertices {
                for gv in g_vertices {
                    let x = translation_x + gv.position.x * text.point;
                    let y = translation_y
                        - atom.line as f32 * self.line_height * text.line_height * text.point
                        + gv.position.y * text.point;
                    vertices.push(Vertex {
                        position: [x, y, 0.0],
                        tex_coords: gv.tex_coords,
                    });
                }

                indices.push(vertex_index);
                indices.push(vertex_index + 3);
                indices.push(vertex_index + 1);

                indices.push(vertex_index + 1);
                indices.push(vertex_index + 3);
                indices.push(vertex_index + 2);

                vertex_index += 4;
            }

            translation_x += atom.scaled_advance;
        }

        Ok(Mesh { vertices, indices })
    }
}

impl TryFrom<FontLayoutData> for FontLayout {
    type Error = anyhow::Error;

    #[inline]
    fn try_from(data: FontLayoutData) -> Result<Self, Self::Error> {
        Self::from_data(data)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Glyph {
    pub advance: f32,

    // 0---1
    // |   |
    // 3---2
    pub vertices: Option<[GlyphVertex; 4]>,
}

impl Glyph {
    const NOOP: Glyph = Glyph {
        advance: 0.0,
        vertices: None,
    };
}

#[derive(Debug, Copy, Clone)]
pub struct GlyphVertex {
    pub position: Point2<f32>,
    pub tex_coords: [f32; 2],
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FontLayoutData {
    atlas: Atlas,
    metrics: Metrics,
    glyphs: Vec<LayoutGlyph>,
    // TODO: kerning
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Atlas {
    size: f32,
    width: f32,
    height: f32,
    distance_range: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metrics {
    line_height: f32,
    ascender: f32,
    descender: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayoutGlyph {
    unicode: u32,
    advance: f32,
    plane_bounds: Option<Bounds>,
    atlas_bounds: Option<Bounds>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bounds {
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize)]
pub enum HorizontalAlignment {
    Left,
    Right,
    Center,
    Justified,
}

impl Default for HorizontalAlignment {
    #[inline]
    fn default() -> Self {
        Self::Center
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

impl Default for VerticalAlignment {
    #[inline]
    fn default() -> Self {
        Self::Center
    }
}

#[derive(Debug, Clone)]
pub struct RawText<S> {
    pub pipeline: AssetId<Pipeline, S>,
    pub font: AssetId<Font, S>,
    // Optimization: Arc?
    pub content: Cow<'static, str>,
    pub position: Point2<f32>,
    pub z_index: f32,
    pub rotation: Rotation2<f32>,
    pub point: f32,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub line_height: f32,
    pub vertical_alignment: VerticalAlignment,
    pub horizontal_alignment: HorizontalAlignment,
    pub scale: f32,
    pub tint: [f32; 3],
    pub world: Similarity2<f32>,
    pub world_z_index: f32,
}

impl<S: Clone> RawText<S> {
    pub(crate) fn to_weak(&self) -> RawText<Weak> {
        RawText {
            pipeline: self.pipeline.to_weak(),
            font: self.font.to_weak(),
            content: self.content.clone(),
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

    fn to_raw_instance(
        &self,
        font_layout_size: f32,
        font_layout_distance_range: f32,
        font_texture: AssetId<Texture, S>,
        text_mesh: AssetId<Mesh, S>,
    ) -> RawInstance<S> {
        RawInstance {
            pipeline: self.pipeline.clone(),
            mesh: text_mesh,
            texture: font_texture,
            texture_layer: 0,
            model: Isometry3::from_parts(
                Translation3::new(self.position.x, self.position.y, self.z_index),
                UnitQuaternion::from_axis_angle(&Vector3::z_axis(), self.rotation.angle()),
            ),
            // TODO: text requires correct scale but the scale factor of world isn't included here
            scale: Vector3::new(
                self.scale,
                self.point / (font_layout_size + font_layout_distance_range),
                font_layout_distance_range,
            ),
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

    pub(crate) fn major_hash(&self) -> u64 {
        let mut hasher = AHasher::default();

        self.font.hash(&mut hasher);
        self.content.hash(&mut hasher);
        self.width
            .unwrap_or_else(|| f32::INFINITY)
            .to_ne_bytes()
            .hash(&mut hasher);
        self.height
            .unwrap_or_else(|| f32::INFINITY)
            .to_ne_bytes()
            .hash(&mut hasher);
        self.line_height.to_ne_bytes().hash(&mut hasher);
        self.vertical_alignment.hash(&mut hasher);
        self.horizontal_alignment.hash(&mut hasher);

        hasher.finish()
    }
}

#[derive(Debug)]
pub struct RealizedText {
    pub(crate) raw: RawText<Weak>,
    pub(crate) mesh: StrongAssetId<Mesh>,
    pub(crate) font_texture: WeakAssetId<Texture>,
    pub(crate) layout: WeakAssetId<FontLayout>,
    pub(crate) font_layout_size: f32,
    pub(crate) font_layout_distance_range: f32,
    pub(crate) canvas_layer_id: Uuid,
}

#[derive(Debug)]
pub struct QueuedText {
    pub(crate) raw: RawText<Weak>,
    pub(crate) layout: Option<WeakAssetId<FontLayout>>,
    pub(crate) canvas_layer_id: Uuid,
}

pub struct Texts {
    loaded: HashMap<Uuid, RealizedText>,
    queued: HashMap<Uuid, QueuedText>,
    font_index: BTreeSet<(WeakAssetId<Font>, OrderWindow<Uuid>)>,
    font_layout_index: BTreeSet<(WeakAssetId<FontLayout>, OrderWindow<Uuid>)>,
    // defaults
    pub(crate) font: StrongAssetId<Font>,
}

impl Texts {
    pub fn new(assets: &AssetsClient) -> anyhow::Result<Self> {
        let font_image = assets.store(
            Font::FONT_IMAGE_UUID,
            image::load_from_memory(include_bytes!(
                "../../asset/font/OpenSans/OpenSans-SemiBold.png"
            ))?,
        );
        let texture = assets.store(
            Font::FONT_TEXTURE_UUID,
            Texture::new(font_image)
                .with_mag_filter(FilterMode::Linear)
                .with_min_filter(FilterMode::Linear),
        );
        let layout = assets.store(
            Font::FONT_LAYOUT_UUID,
            FontLayout::from_bytes(include_bytes!(
                "../../asset/font/OpenSans/OpenSans-SemiBold.json"
            ))?,
        );
        let font = assets.store(Font::FONT_UUID, Font { texture, layout });

        Ok(Self {
            loaded: Default::default(),
            queued: Default::default(),
            font_index: Default::default(),
            font_layout_index: Default::default(),
            font,
        })
    }

    pub fn texts_for_font(&self, font_id: WeakAssetId<Font>) -> Vec<Uuid> {
        use std::ops::Bound::Included;

        self.font_index
            .range((
                Included(&(font_id, OrderWindow::Start)),
                Included(&(font_id, OrderWindow::End)),
            ))
            .filter_map(|(_, t)| t.as_option().copied())
            .collect()
    }

    pub fn texts_for_font_layout(&self, font_layout_id: WeakAssetId<FontLayout>) -> Vec<Uuid> {
        use std::ops::Bound::Included;

        self.font_layout_index
            .range((
                Included(&(font_layout_id, OrderWindow::Start)),
                Included(&(font_layout_id, OrderWindow::End)),
            ))
            .filter_map(|(_, t)| t.as_option().copied())
            .collect()
    }

    pub fn queue_text(
        &mut self,
        canvas_layer_id: Uuid,
        text_id: Uuid,
        raw: RawText<Weak>,
        font_layout_id: Option<WeakAssetId<FontLayout>>,
    ) {
        self.remove_queued_text(text_id);
        self.font_index
            .insert((raw.font, OrderWindow::new(text_id)));
        if let Some(font_layout_id) = font_layout_id {
            self.font_layout_index
                .insert((font_layout_id, OrderWindow::new(text_id)));
        }
        self.queued.insert(
            text_id,
            QueuedText {
                raw,
                layout: font_layout_id,
                canvas_layer_id,
            },
        );
    }

    pub fn get_text(&self, text_id: &Uuid) -> Option<(&RawText<Weak>, Uuid)> {
        self.loaded
            .get(text_id)
            .map(|t| (&t.raw, t.canvas_layer_id))
            .or_else(|| {
                self.queued
                    .get(text_id)
                    .map(|t| (&t.raw, t.canvas_layer_id))
            })
    }

    pub fn upsert_text(
        &mut self,
        assets: &AssetsClient,
        canvas_layer_id: Uuid,
        text_id: Uuid,
        raw: RawText<Weak>,
        font_texture_id: WeakAssetId<Texture>,
        font_layout_id: WeakAssetId<FontLayout>,
        font_layout: &FontLayout,
    ) -> anyhow::Result<RawInstance<Weak>> {
        log::debug!("upsert text: {:?}", text_id);
        self.remove_queued_text(text_id);
        self.remove_loaded_text(text_id);

        // Optimization: move mesh generation from render thread
        let mesh = assets.store(text_id, font_layout.generate_mesh(&raw)?);
        let raw_instance = raw.to_raw_instance(
            font_layout.size,
            font_layout.distance_range,
            font_texture_id,
            mesh.to_weak(),
        );

        self.font_index
            .insert((raw.font, OrderWindow::new(text_id)));
        self.font_layout_index
            .insert((font_layout_id, OrderWindow::new(text_id)));
        let realized = RealizedText {
            raw,
            mesh,
            font_texture: font_texture_id,
            layout: font_layout_id,
            font_layout_size: font_layout.size,
            font_layout_distance_range: font_layout.distance_range,
            canvas_layer_id,
        };
        self.loaded.insert(text_id, realized);

        Ok(raw_instance)
    }

    pub fn minor_update_text(
        &mut self,
        canvas_layer_id: Uuid,
        text_id: &Uuid,
        raw: RawText<Weak>,
    ) -> Option<RawInstance<Weak>> {
        self.loaded.get_mut(text_id).map(|realized| {
            log::debug!("minor update text: {:?}", text_id);
            realized.canvas_layer_id = canvas_layer_id;
            realized.raw = raw;
            realized.raw.to_raw_instance(
                realized.font_layout_size,
                realized.font_layout_distance_range,
                realized.font_texture,
                realized.mesh.to_weak(),
            )
        })
    }

    pub fn remove_text(&mut self, text_id: Uuid) {
        log::debug!("remove text: {:?}", text_id);
        self.remove_queued_text(text_id);
        self.remove_loaded_text(text_id);
    }

    fn remove_queued_text(&mut self, text_id: Uuid) {
        if let Some(queued) = self.queued.remove(&text_id) {
            self.font_index
                .remove(&(queued.raw.font, OrderWindow::new(text_id)));
            if let Some(layout) = queued.layout {
                self.font_layout_index
                    .remove(&(layout, OrderWindow::new(text_id)));
            }
        }
    }

    fn remove_loaded_text(&mut self, text_id: Uuid) {
        if let Some(realized) = self.loaded.remove(&text_id) {
            self.font_index
                .remove(&(realized.raw.font, OrderWindow::new(text_id)));
            self.font_layout_index
                .remove(&(realized.layout, OrderWindow::new(text_id)));
        }
    }
}
