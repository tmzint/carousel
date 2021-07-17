use crate::asset::storage::AssetsClient;
use crate::asset::{AssetId, StrongAssetId, Weak};
use crate::prelude::Texture;
use crate::render::buffer::Vertex;
use crate::render::canvas::RawInstance;
use crate::render::mesh::Mesh;
use crate::render::pipeline::Pipeline;
use crate::util::HashMap;
use ahash::AHasher;
use lyon::tessellation::{
    BuffersBuilder, StrokeOptions as LStrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use nalgebra::{Isometry3, Point2, Rotation2, UnitQuaternion, Vector2, Vector3, Translation3, Similarity3};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use uuid::Uuid;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub enum LineCap {
    Butt,
    Square,
    Round,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub enum LineJoin {
    Miter,
    MiterClip,
    Round,
    Bevel,
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
pub struct StrokeOptions {
    pub start_cap: LineCap,
    pub end_cap: LineCap,
    pub line_join: LineJoin,
    pub line_width: f32,
    pub miter_limit: f32,
    pub tolerance: f32,
}

impl Default for StrokeOptions {
    fn default() -> Self {
        Self {
            start_cap: LineCap::Butt,
            end_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            line_width: 1.0,
            miter_limit: 4.0,
            tolerance: 0.1,
        }
    }
}

impl Hash for StrokeOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.start_cap.hash(state);
        self.end_cap.hash(state);
        self.line_join.hash(state);
        self.line_width.to_ne_bytes().hash(state);
        self.miter_limit.to_ne_bytes().hash(state);
        self.tolerance.to_ne_bytes().hash(state);
    }
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
pub enum Segment {
    Begin(Point2<f32>),
    Line(Point2<f32>),
    End,
    Close,
}

impl Hash for Segment {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Segment::Begin(point) => {
                0.hash(state);
                point.x.to_ne_bytes().hash(state);
                point.y.to_ne_bytes().hash(state);
            }
            Segment::Line(point) => {
                1.hash(state);
                point.x.to_ne_bytes().hash(state);
                point.y.to_ne_bytes().hash(state);
            }
            Segment::End => {
                2.hash(state);
            }
            Segment::Close => {
                3.hash(state);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PathBuilder {
    segments: Vec<Segment>,
}

impl PathBuilder {
    #[inline]
    pub fn begin(mut self, point: Point2<f32>) -> Self {
        self.segments.push(Segment::Begin(point));
        self
    }

    #[inline]
    pub fn line(mut self, point: Point2<f32>) -> Self {
        self.segments.push(Segment::Line(point));
        self
    }

    #[inline]
    pub fn end(mut self) -> Self {
        self.segments.push(Segment::End);
        self
    }

    #[inline]
    pub fn close(mut self) -> Self {
        self.segments.push(Segment::Close);
        self
    }

    #[inline]
    pub fn finalize(self) -> Path {
        Path::new(self.segments)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(from = "Vec<Segment>", into = "Vec<Segment>")]
pub struct Path {
    segments: Vec<Segment>,
    hash: u64,
}

impl Path {
    fn new(segments: Vec<Segment>) -> Self {
        let mut hasher = AHasher::default();
        segments.hash(&mut hasher);
        let hash = hasher.finish();

        Self { segments, hash }
    }

    #[inline]
    pub fn builder() -> PathBuilder {
        PathBuilder {
            segments: Default::default(),
        }
    }

    #[inline]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}

impl From<Vec<Segment>> for Path {
    #[inline]
    fn from(segments: Vec<Segment>) -> Self {
        Self::new(segments)
    }
}

impl Into<Vec<Segment>> for Path {
    #[inline]
    fn into(self) -> Vec<Segment> {
        self.segments
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new(Vec::default())
    }
}

#[derive(Debug, Clone)]
pub struct RawCurve<S> {
    pub pipeline: AssetId<Pipeline, S>,
    pub path: Path,
    pub stroke: StrokeOptions,
    pub position: Point2<f32>,
    pub z_index: f32,
    pub rotation: Rotation2<f32>,
    pub scale: Vector2<f32>,
    pub tint: [f32; 3],
    pub world: Similarity3<f32>
}

impl<S: Clone> RawCurve<S> {
    pub(crate) fn to_weak(&self) -> RawCurve<Weak> {
        RawCurve {
            pipeline: self.pipeline.to_weak(),
            path: self.path.clone(),
            stroke: self.stroke,
            position: self.position,
            z_index: self.z_index,
            rotation: self.rotation,
            scale: self.scale,
            tint: self.tint,
            world: self.world
        }
    }

    fn to_raw_instance(
        &self,
        path_mesh: AssetId<Mesh, S>,
        white_texture: AssetId<Texture, S>,
    ) -> RawInstance<S> {
        RawInstance {
            pipeline: self.pipeline.clone(),
            mesh: path_mesh,
            texture: white_texture,
            texture_layer: 0,
            model: Isometry3::from_parts(
                Translation3::new(self.position.x, self.position.y, self.z_index),
                UnitQuaternion::from_axis_angle(&Vector3::z_axis(), self.rotation.angle()),
            ),
            scale: Vector3::new(self.scale.x, self.scale.y, 1.0),
            tint: self.tint,
            world: self.world
        }
    }

    pub(crate) fn major_hash(&self) -> u64 {
        let mut hasher = AHasher::default();

        self.path.hash.hash(&mut hasher);
        self.stroke.hash(&mut hasher);

        hasher.finish()
    }
}

#[allow(dead_code)]
pub struct RealizedCurve {
    pub(crate) raw: RawCurve<Weak>,
    pub(crate) mesh: StrongAssetId<Mesh>,
    pub(crate) canvas_layer_id: Uuid,
}

pub struct Curves {
    loaded: HashMap<Uuid, RealizedCurve>,
    white_texture: StrongAssetId<Texture>,
}

impl Curves {
    pub fn new(white_texture: StrongAssetId<Texture>) -> Self {
        Self {
            loaded: Default::default(),
            white_texture,
        }
    }

    pub fn upsert_curve(
        &mut self,
        assets: &AssetsClient,
        canvas_layer_id: Uuid,
        curve_id: Uuid,
        raw: RawCurve<Weak>,
    ) -> anyhow::Result<RawInstance<Weak>> {
        log::debug!("upsert curve: {:?}", curve_id);
        // Optimization: share mesh assets for same path
        // Optimization: move mesh generation from render thread

        let path = {
            let mut builder = lyon::path::Path::builder();

            for segment in raw.path.segments() {
                match segment {
                    Segment::Begin(point) => {
                        builder.begin(lyon::geom::point(point.x, point.y));
                    }
                    Segment::Line(point) => {
                        builder.line_to(lyon::geom::point(point.x, point.y));
                    }
                    Segment::End => {
                        builder.end(false);
                    }
                    Segment::Close => {
                        builder.end(true);
                    }
                }
            }

            builder.build()
        };

        let stroke = {
            fn cap_to_lyon(cap: LineCap) -> lyon::tessellation::LineCap {
                match cap {
                    LineCap::Butt => lyon::tessellation::LineCap::Butt,
                    LineCap::Square => lyon::tessellation::LineCap::Square,
                    LineCap::Round => lyon::tessellation::LineCap::Round,
                }
            }

            let line_join = match raw.stroke.line_join {
                LineJoin::Miter => lyon::tessellation::LineJoin::Miter,
                LineJoin::MiterClip => lyon::tessellation::LineJoin::MiterClip,
                LineJoin::Round => lyon::tessellation::LineJoin::Round,
                LineJoin::Bevel => lyon::tessellation::LineJoin::Bevel,
            };

            let mut stroke = LStrokeOptions::default();

            stroke.start_cap = cap_to_lyon(raw.stroke.start_cap);
            stroke.end_cap = cap_to_lyon(raw.stroke.end_cap);
            stroke.line_join = line_join;
            stroke.line_width = raw.stroke.line_width;
            stroke.miter_limit = raw.stroke.miter_limit;
            stroke.tolerance = raw.stroke.tolerance;

            stroke
        };

        let mut geometry = VertexBuffers::new();
        StrokeTessellator::new()
            .tessellate_path(
                path.as_slice(),
                &stroke,
                &mut BuffersBuilder::new(&mut geometry, |v: StrokeVertex| {
                    let pos = v.position();
                    Vertex {
                        position: [pos.x, pos.y, 0.0],
                        tex_coords: [0.0, 0.0],
                    }
                }),
            )
            .map_err(|e| anyhow::anyhow!("Missing attribute: {:?}", e))?;
        geometry.indices.reverse();

        let mesh = assets.store(
            Uuid::new_v4(),
            Mesh {
                vertices: geometry.vertices,
                indices: geometry.indices,
            },
        );
        let raw_instance = raw.to_raw_instance(mesh.to_weak(), self.white_texture.to_weak());

        let realized = RealizedCurve {
            raw,
            mesh,
            canvas_layer_id,
        };
        self.loaded.insert(curve_id, realized);

        Ok(raw_instance)
    }

    pub fn minor_update_curve(
        &mut self,
        canvas_layer_id: Uuid,
        curve_id: &Uuid,
        raw: RawCurve<Weak>,
    ) -> Option<RawInstance<Weak>> {
        let white_texture = self.white_texture.to_weak();
        self.loaded.get_mut(&curve_id).map(|realized| {
            log::debug!("minor update curve: {:?}", curve_id);
            realized.canvas_layer_id = canvas_layer_id;
            realized.raw = raw;
            realized
                .raw
                .to_raw_instance(realized.mesh.to_weak(), white_texture)
        })
    }

    pub fn remove_curve(&mut self, curve_id: &Uuid) {
        log::info!("remove curve: {:?}", curve_id);
        self.loaded.remove(curve_id);
    }
}
