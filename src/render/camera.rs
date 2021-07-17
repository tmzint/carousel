use crate::util::{Counted, HashMap};
use nalgebra::{Isometry3, Matrix4, Orthographic3, Point3, Vector2, Vector3, Point2};
use std::cmp::Ordering;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

const ZFAR: f32 = 20000.0;

#[rustfmt::skip]
const OPENGL_TO_WGPU_MATRIX: Matrix4<f32> = Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.5,
    0.0, 0.0, 0.0, 1.0,
);

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct OrthographicProjection {
    pub rect: Vector2<f32>,
    pub zoom: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl OrthographicProjection {
    #[inline]
    pub fn new(rect: Vector2<f32>) -> Self {
        OrthographicProjection {
            rect,
            znear: 0.001,
            zfar: 20000.0,
            zoom: 1.0,
        }
    }

    #[inline]
    pub fn zoomed(&self) -> Vector2<f32> {
        self.rect / self.zoom
    }

    #[inline]
    pub fn to_homogeneous(&self) -> Matrix4<f32> {
        let half = self.zoomed() / 2.0;
        let proj = Orthographic3::new(
            -half.x as f32,
            half.x as f32,
            -half.y as f32,
            half.y as f32,
            self.znear,
            ZFAR,
        )
        .to_homogeneous();

        // see https://metashapes.com/blog/opengl-metal-projection-matrix-problem/
        OPENGL_TO_WGPU_MATRIX * proj
    }

    #[inline]
    pub fn px_range_factor(&self, base: Vector2<f32>) -> Vector2<f32> {
        let zoomed = self.zoomed();
        Vector2::new(base.x / zoomed.x, base.y / zoomed.y)
    }

    #[inline]
    pub fn scaled(&self, base: Vector2<f32>) -> Self {
        let mut a = self.to_owned();

        let aspect = base.x / base.y;
        let min_aspect = a.rect.x / a.rect.y;
        match min_aspect
            .partial_cmp(&aspect)
            .expect("not NaN for delta_aspect")
        {
            Ordering::Less => {
                a.rect.x = a.rect.x / min_aspect * aspect;
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                let inverse_min_aspect = 1.0 / min_aspect;
                let inverse_aspect = 1.0 / aspect;
                a.rect.y = a.rect.y / inverse_min_aspect * inverse_aspect;
            }
        }

        a
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct RawCamera {
    pub eye: Point2<f32>,
    pub projection: OrthographicProjection,
}

impl RawCamera {
    #[inline]
    pub fn new(rect: Vector2<f32>, eye: Point2<f32>) -> Self {
        let projection = OrthographicProjection::new(rect);
        RawCamera::from_parts(eye, projection)
    }

    #[inline]
    pub fn from_parts(eye: Point2<f32>, projection: OrthographicProjection) -> Self {
        Self { eye, projection }
    }

    #[inline]
    pub fn view(&self) -> Isometry3<f32> {
        Isometry3::look_at_rh(&Point3::new(self.eye.x, self.eye.y, ZFAR / 2.0), &Point3::new(self.eye.x, self.eye.y, - 1.0), &Vector3::y())
    }

    #[inline]
    pub fn relative_to_origin(&self, point: Vector2<f32>) -> Vector2<f32> {
        let scaled = self.projection.zoomed();
        Vector2::new(
            scaled.x * point.x + self.eye.x,
            scaled.y * point.y + self.eye.y,
        )
    }
}

#[derive(Default)]
pub struct Cameras {
    underlying: HashMap<Uuid, Counted<RawCamera>>,
}

impl Cameras {
    pub fn insert_camera(&mut self, id: Uuid, camera: RawCamera) {
        log::debug!("insert camera: {:?}", id);
        self.underlying.insert(id, Counted::one(camera));
    }

    pub fn update_camera(&mut self, id: &Uuid, camera: RawCamera) {
        log::debug!("update camera: {:?}", id);
        *self
            .underlying
            .get_mut(id)
            .expect("camera to update")
            .deref_mut() = camera;
    }

    pub fn inc_camera(&mut self, id: &Uuid) {
        self.underlying.get_mut(id).map(Counted::inc);
    }

    pub fn dec_camera(&mut self, id: &Uuid) {
        let count = self
            .underlying
            .get_mut(id)
            .map(Counted::dec)
            .unwrap_or_default();

        if count == 0 {
            log::debug!("remove camera: {:?}", id);
            self.underlying.remove(id);
        }
    }

    pub fn get(&self, camera_id: &Uuid) -> Option<RawCamera> {
        self.underlying.get(camera_id).map(Deref::deref).copied()
    }
}
