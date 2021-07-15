pub mod buffer;
pub mod camera;
pub mod canvas;
pub mod client;
pub mod curve;
pub mod mesh;
pub mod message;
pub mod pipeline;
pub mod text;
pub mod view;

use crate::asset::storage::Assets;
use crate::asset::{AssetEvent, AssetEventKind, AssetsCreatedEvent};
use crate::platform::message::{DisplayCreatedEvent, DisplayResizedEvent, FrameRequestedEvent};
use crate::render::camera::Cameras;
use crate::render::canvas::Canvasses;
use crate::render::client::RenderDefaults;
use crate::render::curve::Curves;
use crate::render::mesh::{Mesh, Meshes};
use crate::render::message::{
    CameraEvent, CameraEventKind, CanvasEvent, CanvasEventKind, CanvasLayerEvent,
    CanvasLayerEventKind, CurveEvent, CurveEventKind, DrawnEvent, InstanceEvent, InstanceEventKind,
    RenderCreatedEvent, TextEvent, TextEventKind,
};
use crate::render::pipeline::{Pipeline, Pipelines, WGSLSource};
use crate::render::text::{Font, FontLayout, Texts};
use crate::render::view::{RealizedView, Texture, Textures};
use image::DynamicImage;
use roundabout::prelude::*;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Samples {
    Two = 2,
    Four = 4,
    Eight = 8,
}

impl From<Samples> for u32 {
    fn from(samples: Samples) -> Self {
        match samples {
            Samples::Two => 2,
            Samples::Four => 4,
            Samples::Eight => 8,
        }
    }
}

pub struct Renderer {
    size: [u32; 2],
    instance: wgpu::Instance,
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    sc_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    pipelines: Pipelines,
    textures: Textures,
    meshes: Meshes,
    cameras: Cameras,
    canvasses: Canvasses,
    texts: Texts,
    curves: Curves,
}

impl Renderer {
    pub async fn new(
        assets: &Assets,
        sender: &MessageSender,
        size: [u32; 2],
        instance: wgpu::Instance,
        surface: wgpu::Surface,
        samples: Samples,
    ) -> anyhow::Result<Self> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("Missing gpu adapter"))?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await?;

        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: RealizedView::FRAME_TEXTURE_FORMAT,
            width: size[0],
            height: size[1],
            present_mode: wgpu::PresentMode::Mailbox,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("uniform_bind_group_layout"),
            });

        let diffuse_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            filtering: true,
                            comparison: false,
                        },
                        count: None,
                    },
                ],
                label: Some("diffuse_texture_bind_group_layout"),
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&diffuse_bind_group_layout, &uniform_bind_group_layout],
                push_constant_ranges: &[],
            });

        let assets = assets.client();
        let pipelines = Pipelines::new(&assets, render_pipeline_layout, samples);
        let textures = Textures::new(&assets, diffuse_bind_group_layout);
        let meshes = Meshes::new(&assets);
        let cameras = Cameras::default();
        let canvasses = Canvasses::new(uniform_bind_group_layout, size, samples);
        let texts = Texts::new(&assets)?;
        let curves = Curves::new(textures.white_texture.clone());

        let render_defaults = RenderDefaults::new(&pipelines, &textures, &meshes, &texts)?;
        sender.send(RenderCreatedEvent {
            defaults: Box::new(render_defaults),
        });

        Ok(Renderer {
            size,
            instance,
            surface,
            device,
            queue,
            sc_desc,
            swap_chain,
            pipelines,
            textures,
            meshes,
            cameras,
            canvasses,
            texts,
            curves,
        })
    }

    pub fn resize(&mut self, size: [u32; 2]) {
        self.size = size;
        self.sc_desc.width = size[0];
        self.sc_desc.height = size[1];
        self.swap_chain = self.device.create_swap_chain(&self.surface, &self.sc_desc);
        self.canvasses.resize(&self.device, self.size);
    }

    pub fn render(&mut self) -> anyhow::Result<()> {
        self.canvasses.draw(
            &self.device,
            &self.queue,
            &self.swap_chain,
            &self.cameras,
            &self.pipelines,
            &self.textures,
            &self.meshes,
        )
    }
}

#[derive(Default)]
pub struct RenderServer {
    assets: Option<Assets>,
    renderer: Option<Renderer>,
}

impl RenderServer {
    pub fn new(
        handler: OpenMessageHandlerBuilder<RenderServer>,
    ) -> InitMessageHandlerBuilder<RenderServer> {
        // TODO: move event handler functions into sub modules
        handler
            .on(on_assets_created_event)
            .on(on_display_created_event)
            .on(on_display_resized_event)
            .on(on_camera_event)
            .on(on_canvas_layer_event)
            .on(on_canvas_event)
            .on(on_wgsl_source_asset_event)
            .on(on_image_asset_event)
            .on(on_texture_asset_event)
            .on(on_pipeline_asset_event)
            .on(on_mesh_asset_event)
            .on(on_instance_event)
            .on(on_font_layout_asset_event)
            .on(on_font_asset_event)
            .on(on_text_event)
            .on(on_curve_event)
            .on(on_frame_requested_event)
            .init_default()
    }
}

fn on_assets_created_event(
    state: &mut RenderServer,
    context: &mut RuntimeContext,
    event: &AssetsCreatedEvent,
) {
    assert!(state.assets.is_none());
    state.assets = Some(event.assets(context.sender().to_owned()));
}

fn on_display_created_event(
    state: &mut RenderServer,
    context: &mut RuntimeContext,
    event: &DisplayCreatedEvent,
) {
    assert!(state.renderer.is_none());

    let render_resources = event
        .render_resources
        .lock()
        .take()
        .expect("render resources");

    let assets = state
        .assets
        .as_ref()
        .expect("assets before display was created");

    let renderer = futures::executor::block_on(Renderer::new(
        assets,
        context.sender(),
        event.window_size,
        render_resources.instance,
        render_resources.window_surface,
        Samples::Two,
    ))
    .expect("renderer creation");

    state.renderer = Some(renderer);
}

fn on_display_resized_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &DisplayResizedEvent,
) {
    if let Some(renderer) = &mut state.renderer {
        renderer.resize(event.size);
    }
}

fn on_camera_event(state: &mut RenderServer, _context: &mut RuntimeContext, event: &CameraEvent) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before camera");

    match event.kind {
        CameraEventKind::Created => {
            renderer.cameras.insert_camera(event.id, event.raw);
        }
        CameraEventKind::Modified => {
            renderer.cameras.update_camera(&event.id, event.raw);
        }
        CameraEventKind::Dropped => {
            renderer.cameras.dec_camera(&event.id);
        }
    };
}

fn on_canvas_layer_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &CanvasLayerEvent,
) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before camera");

    match event.kind {
        CanvasLayerEventKind::Created => {
            renderer
                .canvasses
                .insert_canvas_layer(&renderer.device, event.id);
        }
        CanvasLayerEventKind::Dropped => {
            renderer.canvasses.remove_canvas_layer(&event.id);
        }
    };
}

fn on_canvas_event(state: &mut RenderServer, _context: &mut RuntimeContext, event: &CanvasEvent) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before camera");

    match &event.kind {
        CanvasEventKind::Created(created) => {
            let created = created
                .lock()
                .take()
                .expect("canvas created on canvas creation");
            renderer.canvasses.upsert_canvas(
                &renderer.device,
                &mut renderer.cameras,
                event.id,
                created.size,
                created.priority,
                created.frame,
                created.frames,
            );
        }
        CanvasEventKind::Dropped => {
            renderer
                .canvasses
                .remove_canvas(&mut renderer.cameras, &event.id);
        }
    };
}

fn on_wgsl_source_asset_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<WGSLSource>,
) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before wgsl");

    match event.kind {
        AssetEventKind::Load => {
            let assets = state.assets.as_ref().unwrap().client();
            if let Some(source) = assets.get(&event.id) {
                renderer
                    .pipelines
                    .upsert_shader(&renderer.device, event.id, source);

                for pipeline_id in renderer.pipelines.pipelines_for_shader(event.id) {
                    if let Some(pipeline) = assets.get(&pipeline_id) {
                        if !renderer.pipelines.upsert_pipeline(
                            &renderer.device,
                            pipeline_id,
                            pipeline,
                        ) {
                            renderer.pipelines.queue_pipeline(pipeline_id, pipeline);
                        }
                    }
                }
            }
        }
        AssetEventKind::Unload => {
            renderer.pipelines.remove_shader(&event.id);
        }
    };
}

fn on_pipeline_asset_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<Pipeline>,
) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before pipeline");

    match event.kind {
        AssetEventKind::Load => {
            let assets = state.assets.as_mut().unwrap();
            if let Some(pipeline) = assets.client().get(&event.id) {
                if !renderer
                    .pipelines
                    .upsert_pipeline(&renderer.device, event.id, pipeline)
                {
                    renderer.pipelines.queue_pipeline(event.id, pipeline);
                }
            }
        }
        AssetEventKind::Unload => renderer.pipelines.remove_pipeline(event.id),
    }
}

fn on_image_asset_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<DynamicImage>,
) {
    if AssetEventKind::Load == event.kind {
        let renderer = state
            .renderer
            .as_mut()
            .expect("render to be available before image");

        let assets = state.assets.as_ref().unwrap().client();
        if let Some(image) = assets.get(&event.id) {
            for texture_id in renderer.textures.textures_for_image(event.id) {
                if let Some(texture) = assets.get(&texture_id) {
                    renderer.textures.upsert_texture(
                        &renderer.device,
                        &renderer.queue,
                        texture_id,
                        texture,
                        image,
                    );
                }
            }
        }
    }
}

fn on_texture_asset_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<Texture>,
) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before texture");

    match event.kind {
        AssetEventKind::Load => {
            let assets = state.assets.as_ref().unwrap().client();
            if let Some(texture) = assets.get(&event.id) {
                if let Some(image) = assets.get(&texture.image) {
                    renderer.textures.upsert_texture(
                        &renderer.device,
                        &renderer.queue,
                        event.id,
                        texture,
                        image,
                    );
                } else {
                    renderer.textures.queue_texture(event.id, texture);
                }
            }
        }
        AssetEventKind::Unload => {
            renderer.textures.remove_texture(event.id);
        }
    };
}

fn on_mesh_asset_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<Mesh>,
) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before mesh");

    match event.kind {
        AssetEventKind::Load => {
            let assets = state.assets.as_mut().unwrap();
            if let Some(mesh) = assets.client().get(&event.id) {
                renderer
                    .meshes
                    .upsert_mesh(&renderer.device, event.id, mesh);
            }
        }
        AssetEventKind::Unload => {
            renderer.meshes.remove_mesh(&event.id);
        }
    };
}

fn on_instance_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &InstanceEvent,
) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before instance");

    match &event.kind {
        InstanceEventKind::Created(raw_instance) => {
            renderer.canvasses.upsert_instance(
                &renderer.device,
                &event.layer,
                event.id,
                *raw_instance.deref(),
            );
        }
        InstanceEventKind::Modified(raw_instance) => {
            renderer.canvasses.upsert_instance(
                &renderer.device,
                &event.layer,
                event.id,
                *raw_instance.deref(),
            );
        }
        InstanceEventKind::Dropped => {
            renderer.canvasses.remove_instance(&event.layer, &event.id);
        }
    }
}

fn on_font_layout_asset_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<FontLayout>,
) {
    if event.kind == AssetEventKind::Load {
        let renderer = state
            .renderer
            .as_mut()
            .expect("render to be available before font");

        let assets = state.assets.as_ref().unwrap().client();
        if let Some(font_layout) = assets.get(&event.id) {
            for text_id in renderer.texts.texts_for_font_layout(event.id) {
                let (raw_text, canvas_layer_id) = renderer
                    .texts
                    .get_text(&text_id)
                    .map(|(b, cl)| (b.to_owned(), cl))
                    .expect("text");

                if let Some(font) = assets.get(&raw_text.font) {
                    let raw_instance = renderer
                        .texts
                        .upsert_text(
                            &assets,
                            canvas_layer_id,
                            text_id,
                            raw_text,
                            font.texture.to_weak(),
                            event.id,
                            font_layout,
                        )
                        .expect("upsert text");

                    renderer.canvasses.upsert_instance(
                        &renderer.device,
                        &canvas_layer_id,
                        text_id,
                        raw_instance,
                    );
                }
            }
        }
    }
}

fn on_font_asset_event(
    state: &mut RenderServer,
    _context: &mut RuntimeContext,
    event: &AssetEvent<Font>,
) {
    if event.kind == AssetEventKind::Load {
        let renderer = state
            .renderer
            .as_mut()
            .expect("render to be available before font");

        let assets = state.assets.as_ref().unwrap().client();
        if let Some(font) = assets.get(&event.id) {
            for text_id in renderer.texts.texts_for_font(event.id) {
                let (raw_text, canvas_layer_id) = renderer
                    .texts
                    .get_text(&text_id)
                    .map(|(b, cl)| (b.to_owned(), cl))
                    .expect("text");

                if let Some(font_layout) = assets.get(&font.layout) {
                    let instance = renderer
                        .texts
                        .upsert_text(
                            &assets,
                            canvas_layer_id,
                            text_id,
                            raw_text,
                            font.texture.to_weak(),
                            font.layout.to_weak(),
                            font_layout,
                        )
                        .expect("upsert text");

                    renderer.canvasses.upsert_instance(
                        &renderer.device,
                        &canvas_layer_id,
                        text_id,
                        instance,
                    );
                }
            }
        }
    }
}

fn on_text_event(state: &mut RenderServer, _context: &mut RuntimeContext, event: &TextEvent) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before text");

    let raw_text = match &event.kind {
        TextEventKind::Created(raw) => raw,
        TextEventKind::Modified {
            raw,
            major_change: true,
        } => raw,
        TextEventKind::Modified {
            raw,
            major_change: false,
        } => {
            if let Some(raw_instance) =
                renderer
                    .texts
                    .minor_update_text(event.layer, &event.id, raw.deref().to_owned())
            {
                renderer.canvasses.upsert_instance(
                    &renderer.device,
                    &event.layer,
                    event.id,
                    raw_instance,
                );
            }

            return;
        }
        TextEventKind::Dropped => {
            renderer.texts.remove_text(event.id);
            renderer.canvasses.remove_instance(&event.layer, &event.id);
            return;
        }
    };

    let assets = state.assets.as_ref().unwrap().client();
    let font = assets.get(&raw_text.font);
    let font_layout = font.and_then(|f| assets.get(&f.layout));

    if let (Some(font), Some(font_layout)) = (font, font_layout) {
        let raw_instance = renderer
            .texts
            .upsert_text(
                &assets,
                event.layer,
                event.id,
                raw_text.deref().to_owned(),
                font.texture.to_weak(),
                font.layout.to_weak(),
                font_layout,
            )
            .expect("upsert text");

        renderer
            .canvasses
            .upsert_instance(&renderer.device, &event.layer, event.id, raw_instance);
    } else {
        renderer.texts.queue_text(
            event.layer,
            event.id,
            raw_text.deref().to_owned(),
            font.map(|f| f.layout.to_weak()),
        )
    }
}

fn on_curve_event(state: &mut RenderServer, _context: &mut RuntimeContext, event: &CurveEvent) {
    let renderer = state
        .renderer
        .as_mut()
        .expect("render to be available before curve");

    let raw_curve = match &event.kind {
        CurveEventKind::Created(raw) => raw,
        CurveEventKind::Modified {
            raw,
            major_change: true,
        } => raw,
        CurveEventKind::Modified {
            raw,
            major_change: false,
        } => {
            if let Some(raw_instance) =
                renderer
                    .curves
                    .minor_update_curve(event.layer, &event.id, raw.deref().to_owned())
            {
                renderer.canvasses.upsert_instance(
                    &renderer.device,
                    &event.layer,
                    event.id,
                    raw_instance,
                );
            }

            return;
        }
        CurveEventKind::Dropped => {
            renderer.curves.remove_curve(&event.id);
            renderer.canvasses.remove_instance(&event.layer, &event.id);
            return;
        }
    };

    let assets = state.assets.as_ref().unwrap().client();
    let raw_instance = renderer
        .curves
        .upsert_curve(&assets, event.layer, event.id, raw_curve.deref().to_owned())
        .expect("upsert curve");

    renderer
        .canvasses
        .upsert_instance(&renderer.device, &event.layer, event.id, raw_instance);
}

fn on_frame_requested_event(
    state: &mut RenderServer,
    context: &mut RuntimeContext,
    event: &FrameRequestedEvent,
) {
    if let Some(renderer) = &mut state.renderer {
        renderer.render().expect("render");
    }

    context.sender().send(DrawnEvent { frame: event.frame });
}
