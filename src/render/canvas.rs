use crate::asset::{AssetId, Weak, WeakAssetId};
use crate::render::buffer::{Instance, Uniforms};
use crate::render::camera::{Cameras, RawCamera};
use crate::render::mesh::{Mesh, Meshes};
use crate::render::pipeline::{Pipeline, Pipelines};
use crate::render::view::{RealizedView, Texture, Textures};
use crate::render::Samples;
use crate::some_or_continue;
use crate::util::{Counted, HashMap, IndexMap};
use nalgebra::{Isometry3, Vector2, Vector3, Similarity3};
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use uuid::Uuid;
use wgpu::util::DeviceExt;

#[derive(Debug, Copy, Clone)]
pub struct RawInstance<S> {
    pub pipeline: AssetId<Pipeline, S>,
    pub mesh: AssetId<Mesh, S>,
    pub texture: AssetId<Texture, S>,
    pub texture_layer: u32,
    pub model: Isometry3<f32>,
    pub scale: Vector3<f32>,
    pub tint: [f32; 3],
    pub world: Similarity3<f32>
}

impl<S> RawInstance<S> {
    pub(crate) fn to_weak(&self) -> RawInstance<Weak> {
        RawInstance {
            pipeline: self.pipeline.to_weak(),
            mesh: self.mesh.to_weak(),
            texture: self.texture.to_weak(),
            texture_layer: self.texture_layer,
            model: self.model,
            scale: self.scale,
            tint: self.tint,
            world: self.world
        }
    }
}

#[derive(Debug)]
struct InstanceEntry {
    raw: RawInstance<Weak>,
    buffer_index: u64,
    buffer_offset: usize,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
struct RenderKey {
    pipeline: WeakAssetId<Pipeline>,
    mesh: WeakAssetId<Mesh>,
    texture: WeakAssetId<Texture>,
    buffer_index: u64,
}

struct RenderEntry {
    instance_buffer: wgpu::Buffer,
    capacity: u32,
    instances: Vec<Uuid>,
}

pub struct RealizedCanvasLayer {
    uniform_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    instance_index: BTreeMap<Uuid, InstanceEntry>,
    render_index: BTreeMap<RenderKey, RenderEntry>,
    buffer_counter: u64,
}

impl RealizedCanvasLayer {
    pub fn new(device: &wgpu::Device, uniform_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[Uniforms::default()]),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
            label: Some("uniform_bind_group"),
        });

        Self {
            uniform_bind_group,
            uniform_buffer,
            instance_index: Default::default(),
            render_index: Default::default(),
            buffer_counter: 0,
        }
    }

    pub fn upsert_instance(
        &mut self,
        device: &wgpu::Device,
        instance_id: Uuid,
        raw: RawInstance<Weak>,
    ) {
        // Optimization: batching

        let buffer_index = self.buffer_counter;
        self.buffer_counter += 1;
        let instance_entry = InstanceEntry {
            raw,
            buffer_index,
            buffer_offset: 0,
        };

        match self.instance_index.entry(instance_id) {
            Entry::Occupied(mut e) => {
                let current = e.get_mut();

                // Optimization: here vs modify
                if &current.raw.model == &instance_entry.raw.model
                    && &current.raw.tint == &instance_entry.raw.tint
                    && &current.raw.scale == &instance_entry.raw.scale
                    && &current.raw.texture == &instance_entry.raw.texture
                    && &current.raw.mesh == &instance_entry.raw.mesh
                    && &current.raw.pipeline == &instance_entry.raw.pipeline
                {
                    // identical, no changes needed
                    return;
                }

                let prev = std::mem::replace(current, instance_entry);

                self.render_index.remove(&RenderKey {
                    pipeline: prev.raw.pipeline,
                    mesh: prev.raw.mesh,
                    texture: prev.raw.texture,
                    buffer_index: prev.buffer_index,
                });
            }
            Entry::Vacant(e) => {
                e.insert(instance_entry);
            }
        };

        // TODO: update partial buffer, encoder.copy_buffer_to_buffer(..)
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&vec![Instance {
                model: (raw.world * raw.model).to_homogeneous().into(),
                scale: (raw.scale * raw.world.scaling()).into(),
                tint: raw.tint,
                texture_layer: raw.texture_layer as i32,
            }]),
            usage: wgpu::BufferUsage::VERTEX,
        });
        let render_entry = RenderEntry {
            instance_buffer,
            capacity: 1,
            instances: vec![instance_id],
        };
        let render_key = RenderKey {
            pipeline: raw.pipeline,
            mesh: raw.mesh,
            texture: raw.texture,
            buffer_index,
        };
        self.render_index.insert(render_key, render_entry);
    }

    pub fn remove_instance(&mut self, instance_id: &Uuid) {
        if let Some(instance) = self.instance_index.remove(instance_id) {
            self.render_index.remove(&RenderKey {
                pipeline: instance.raw.pipeline,
                mesh: instance.raw.mesh,
                texture: instance.raw.texture,
                buffer_index: instance.buffer_index,
            });
        }
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        pipelines: &Pipelines,
        textures: &Textures,
        meshes: &Meshes,
        target: Option<&wgpu::SwapChainTexture>,
        attachment: &RealizedView,
        color_load_ops: wgpu::LoadOp<wgpu::Color>,
        depth_texture_view: &wgpu::TextureView,
        depth_load_ops: wgpu::LoadOp<f32>,
        encoder: &mut wgpu::CommandEncoder,
        camera: RawCamera,
    ) -> anyhow::Result<()> {
        // Optimization: culling

        let projection_base =
            Vector2::new(attachment.size.width as f32, attachment.size.height as f32);
        let projection_scaled = camera.projection.scaled(projection_base);
        let uniforms = Uniforms {
            camera_view: camera.view().to_homogeneous().into(),
            camera_proj: projection_scaled.to_homogeneous().into(),
            px_range_factor: projection_scaled.px_range_factor(projection_base).x,
        };
        let update_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Update Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsage::COPY_SRC,
        });
        encoder.copy_buffer_to_buffer(
            &update_uniform_buffer,
            0,
            &self.uniform_buffer,
            0,
            std::mem::size_of::<Uniforms>() as _,
        );

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &attachment.view,
                    resolve_target: target.map(|sct| &sct.view),
                    ops: wgpu::Operations {
                        load: color_load_ops,
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load_ops,
                        store: true,
                    }),
                    stencil_ops: None,
                }),
                label: None,
            });

            let mut curr_pipeline: Option<&WeakAssetId<Pipeline>> = None;

            // Optimization: batching
            for (render_key, entry) in &self.render_index {
                // Optimization: add pipeline swap instructions to render index

                match curr_pipeline {
                    Some(op) if &render_key.pipeline == op => {}
                    _ => {
                        // pipeline changed
                        let pipeline =
                            some_or_continue!(pipelines.get_pipeline(&render_key.pipeline));
                        render_pass.set_pipeline(&pipeline.render_pipeline);
                        render_pass.set_bind_group(1, &self.uniform_bind_group, &[]);
                        curr_pipeline = Some(&render_key.pipeline);
                    }
                };

                let realized_texture = some_or_continue!(textures.get_texture(&render_key.texture));
                render_pass.set_bind_group(0, &realized_texture.bind_group, &[]);

                let realized_mesh = some_or_continue!(meshes.get_mesh(&render_key.mesh));
                render_pass.set_vertex_buffer(0, realized_mesh.vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(1, entry.instance_buffer.slice(..));
                render_pass.set_index_buffer(
                    realized_mesh.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                render_pass.draw_indexed(
                    0..realized_mesh.index_length,
                    0,
                    0..entry.instances.len() as _,
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CanvasFrame<'a> {
    Cover {
        layer: Uuid,
        camera: Uuid,
        clear_color: [f64; 4],
        _pd: PhantomData<&'a ()>,
    },
    Merge {
        layer: Uuid,
        camera: Uuid,
        _pd: PhantomData<&'a ()>,
    },
    Stack {
        layer: Uuid,
        camera: Uuid,
        _pd: PhantomData<&'a ()>,
    },
}

impl<'a> CanvasFrame<'a> {
    pub(crate) fn into_static(self) -> CanvasFrame<'static> {
        match self {
            CanvasFrame::Cover {
                layer,
                camera,
                clear_color,
                ..
            } => CanvasFrame::Cover {
                layer,
                camera,
                clear_color,
                _pd: Default::default(),
            },
            CanvasFrame::Merge { layer, camera, .. } => CanvasFrame::Merge {
                layer,
                camera,
                _pd: Default::default(),
            },
            CanvasFrame::Stack { layer, camera, .. } => CanvasFrame::Stack {
                layer,
                camera,
                _pd: Default::default(),
            },
        }
    }

    fn layer(&self) -> Uuid {
        match self {
            CanvasFrame::Cover { layer, .. } => *layer,
            CanvasFrame::Merge { layer, .. } => *layer,
            CanvasFrame::Stack { layer, .. } => *layer,
        }
    }

    fn camera(&self) -> Uuid {
        match self {
            CanvasFrame::Cover { camera, .. } => *camera,
            CanvasFrame::Merge { camera, .. } => *camera,
            CanvasFrame::Stack { camera, .. } => *camera,
        }
    }
}

pub struct RealizedCanvas {
    frame_buffer: RealizedView,
    depth_buffer: RealizedView,
    frames: Vec<CanvasFrame<'static>>,
    swap_chain_sized: bool,
    priority: usize,
    frame: bool,
}

impl RealizedCanvas {
    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        priority: usize,
        frame: bool,
        frames: Vec<CanvasFrame<'static>>,
        samples: u32,
        swap_chain_sized: bool,
    ) -> Self {
        let frame_buffer = RealizedView::frame_buffer(device, size, samples, Some("frame_buffer"));
        let depth_buffer = RealizedView::depth_buffer(device, size, samples, Some("depth_buffer"));

        Self {
            frame_buffer,
            depth_buffer,
            frames,
            swap_chain_sized,
            priority,
            frame,
        }
    }
}

pub struct Canvasses {
    canvasses: IndexMap<Uuid, RealizedCanvas>,
    layers: HashMap<Uuid, Counted<RealizedCanvasLayer>>,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    samples: Samples,
    swap_chain_size: [u32; 2],
}

impl Canvasses {
    pub fn new(
        uniform_bind_group_layout: wgpu::BindGroupLayout,
        swap_chain_size: [u32; 2],
        samples: Samples,
    ) -> Self {
        Self {
            canvasses: Default::default(),
            layers: Default::default(),
            uniform_bind_group_layout,
            swap_chain_size,
            samples,
        }
    }

    pub fn insert_canvas_layer(&mut self, device: &wgpu::Device, canvas_layer_id: Uuid) {
        log::debug!("insert canvas layer: {:?}", canvas_layer_id);
        let layer = RealizedCanvasLayer::new(device, &self.uniform_bind_group_layout);
        assert!(self
            .layers
            .insert(canvas_layer_id, Counted::one(layer))
            .is_none());
    }

    pub fn remove_canvas_layer(&mut self, canvas_layer_id: &Uuid) {
        let count = self
            .layers
            .get_mut(canvas_layer_id)
            .map(Counted::dec)
            .unwrap_or_default();

        if count == 0 {
            log::debug!("remove canvas layer: {:?}", canvas_layer_id);
            self.layers.remove(canvas_layer_id);
        }
    }

    pub fn upsert_canvas(
        &mut self,
        device: &wgpu::Device,
        cameras: &mut Cameras,
        canvas_id: Uuid,
        size: Option<[u32; 2]>,
        priority: usize,
        frame: bool,
        frames: Vec<CanvasFrame<'static>>,
    ) {
        log::debug!("upsert canvas: {:?}", canvas_id);
        for frame in &frames {
            // TODO:
            //  there seems to be a racing condition where the layer is not available
            //  reproducible with the hello_world example by not having a state transition during setup
            //  Might also come from the MessageSender Triple buffer racing condition that results in a different order (prob not)?
            self.layers
                .get_mut(&frame.layer())
                .expect("referenced canvas layer")
                .inc();

            cameras.inc_camera(&frame.camera());
        }

        let realized = RealizedCanvas::new(
            device,
            size.unwrap_or_else(|| self.swap_chain_size),
            priority,
            frame,
            frames,
            self.samples.into(),
            size.is_none(),
        );

        if let Some(prev) = self.canvasses.insert(canvas_id, realized) {
            for frame in &prev.frames {
                self.remove_canvas_layer(&frame.layer());
                cameras.dec_camera(&frame.camera());
            }
        }

        self.canvasses
            .sort_by(|_, v1, _, v2| v1.priority.cmp(&v2.priority));
    }

    pub fn remove_canvas(&mut self, cameras: &mut Cameras, canvas_id: &Uuid) {
        if let Some(prev) = self.canvasses.remove(canvas_id) {
            log::debug!("remove canvas: {:?}", canvas_id);
            for frame in &prev.frames {
                self.remove_canvas_layer(&frame.layer());
                cameras.dec_camera(&frame.camera());
            }
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.swap_chain_size == size {
            return;
        }

        log::debug!(
            "resize frame canvas from {:?} to {:?}",
            self.swap_chain_size,
            size
        );

        let resize_canvas_ids = self
            .canvasses
            .iter()
            .filter_map(|(id, c)| c.swap_chain_sized.then(|| *id))
            .collect::<Vec<_>>();

        for resize_canvas_id in resize_canvas_ids {
            let (priority, frame, frames) = {
                let canvas = self.canvasses.get_mut(&resize_canvas_id).unwrap();
                (
                    canvas.priority,
                    canvas.frame,
                    std::mem::take(&mut canvas.frames),
                )
            };

            let realized = RealizedCanvas::new(
                device,
                size,
                priority,
                frame,
                frames,
                self.samples.into(),
                true,
            );

            assert!(self.canvasses.insert(resize_canvas_id, realized).is_some());
        }

        self.swap_chain_size = size;
    }

    pub fn upsert_instance(
        &mut self,
        device: &wgpu::Device,
        layer_id: &Uuid,
        instance_id: Uuid,
        raw: RawInstance<Weak>,
    ) {
        if let Some(layer) = self.layers.get_mut(layer_id) {
            log::debug!("upsert instance: {:?}", instance_id);
            layer.upsert_instance(device, instance_id, raw);
        }
    }

    pub fn remove_instance(&mut self, layer_id: &Uuid, instance_id: &Uuid) {
        if let Some(layer) = self.layers.get_mut(layer_id) {
            log::debug!("remove instance: {:?}", instance_id);
            layer.remove_instance(instance_id);
        }
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        swap_chain: &wgpu::SwapChain,
        cameras: &Cameras,
        pipelines: &Pipelines,
        textures: &Textures,
        meshes: &Meshes,
    ) -> anyhow::Result<()> {
        let frame = swap_chain.get_current_frame()?.output;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        for (_, canvas) in &mut self.canvasses {
            for canvas_frame in &mut canvas.frames {
                let (layer_id, camera_id, color_load_ops, depth_load_ops) = match canvas_frame {
                    CanvasFrame::Cover {
                        layer,
                        camera,
                        clear_color: [r, g, b, a],
                        ..
                    } => {
                        let color = wgpu::Color {
                            r: *r,
                            g: *g,
                            b: *b,
                            a: *a,
                        };
                        (
                            layer,
                            camera,
                            wgpu::LoadOp::Clear(color),
                            wgpu::LoadOp::Clear(1.0),
                        )
                    }
                    CanvasFrame::Merge { layer, camera, .. } => {
                        (layer, camera, wgpu::LoadOp::Load, wgpu::LoadOp::Load)
                    }
                    CanvasFrame::Stack { layer, camera, .. } => {
                        (layer, camera, wgpu::LoadOp::Load, wgpu::LoadOp::Clear(1.0))
                    }
                };

                let camera = cameras
                    .get(camera_id)
                    .ok_or_else(|| anyhow::anyhow!("Camera not found: {:?}", layer_id))?;

                let layer = self
                    .layers
                    .get_mut(layer_id)
                    .ok_or_else(|| anyhow::anyhow!("Canvas layer not found: {:?}", layer_id))?;

                layer.draw(
                    device,
                    pipelines,
                    textures,
                    meshes,
                    canvas.frame.then(|| &frame),
                    &canvas.frame_buffer,
                    color_load_ops,
                    &canvas.depth_buffer.view,
                    depth_load_ops,
                    &mut encoder,
                    camera,
                )?;
            }
        }

        queue.submit(std::iter::once(encoder.finish()));

        Ok(())
    }
}
