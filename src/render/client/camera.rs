use crate::render::camera::RawCamera;
use crate::render::message::{CameraEvent, CameraEventKind};
use nalgebra::{Point2, Vector2};
use roundabout::prelude::MessageSender;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

#[derive(Debug)]
pub struct Camera {
    id: Uuid,
    raw: RawCamera,
    sender: MessageSender,
}

impl Camera {
    pub(crate) fn new(rect: Vector2<f32>, eye: Point2<f32>, sender: MessageSender) -> Self {
        let camera = Self {
            id: Uuid::new_v4(),
            raw: RawCamera::new(rect, eye),
            sender,
        };

        camera.sender.send(CameraEvent {
            id: camera.id,
            raw: camera.raw,
            kind: CameraEventKind::Created,
        });

        camera
    }

    #[inline]
    pub fn id(&self) -> Uuid {
        self.id
    }

    #[inline]
    pub fn modify(&mut self) -> CameraModify {
        CameraModify(self)
    }

    #[inline]
    pub fn relative_to_world(&self, transform: Vector2<f64>, base: Vector2<f32>) -> Point2<f64> {
        let scaled = self.projection.scaled(base);
        Point2::new(
            scaled.rect.x as f64 * transform.x + self.eye.x as f64,
            scaled.rect.y as f64 * transform.y + self.eye.y as f64,
        )
    }
}

impl Deref for Camera {
    type Target = RawCamera;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl Drop for Camera {
    #[inline]
    fn drop(&mut self) {
        self.sender.send(CameraEvent {
            id: self.id,
            raw: self.raw,
            kind: CameraEventKind::Dropped,
        });
    }
}

#[derive(Debug)]
pub struct CameraModify<'a>(&'a mut Camera);

impl<'a> Deref for CameraModify<'a> {
    type Target = RawCamera;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0.raw
    }
}

impl<'a> DerefMut for CameraModify<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.raw
    }
}

impl<'a> Drop for CameraModify<'a> {
    #[inline]
    fn drop(&mut self) {
        self.0.sender.send(CameraEvent {
            id: self.0.id,
            raw: self.0.raw,
            kind: CameraEventKind::Modified,
        });
    }
}
