use crate::asset::loader::{AssetCursor, AssetLoader};
use crate::asset::storage::AssetsClient;
use crate::asset::{StrongAssetId, WeakAssetId};
use crate::render::buffer::{Instance, Vertex};
use crate::render::view::RealizedView;
use crate::render::Samples;
use crate::some_or_return;
use crate::util::{HashMap, OrderWindow};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::marker::PhantomData;
use uuid::Uuid;

#[derive(Debug, Copy, Clone)]
pub struct EmptyPipelineBuilder;

#[derive(Debug, Copy, Clone)]
pub struct VSPipelineBuilder;

#[derive(Debug, Copy, Clone)]
pub struct CompletePipelineBuilder;

#[derive(Debug, Clone)]
pub struct PipelineBuilder<TS> {
    pub(crate) vs_source: Option<StrongAssetId<WGSLSource>>,
    pub(crate) fs_source: Option<StrongAssetId<WGSLSource>>,
    pub(crate) color_blend: wgpu::BlendComponent,
    pub(crate) alpha_blend: wgpu::BlendComponent,
    pub(crate) priority: usize,
    _pd: PhantomData<TS>,
}

impl<T> PipelineBuilder<T> {
    #[inline]
    pub fn with_priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }

    #[inline]
    pub fn with_color_blend(mut self, color_blend: wgpu::BlendComponent) -> Self {
        self.color_blend = color_blend;
        self
    }

    #[inline]
    pub fn with_alpha_blend(mut self, alpha_blend: wgpu::BlendComponent) -> Self {
        self.alpha_blend = alpha_blend;
        self
    }
}

impl PipelineBuilder<EmptyPipelineBuilder> {
    #[inline]
    pub fn with_vs_source(
        self,
        vs_source: StrongAssetId<WGSLSource>,
    ) -> PipelineBuilder<VSPipelineBuilder> {
        PipelineBuilder {
            vs_source: Some(vs_source),
            fs_source: self.fs_source,
            color_blend: self.color_blend,
            alpha_blend: self.alpha_blend,
            priority: self.priority,
            _pd: Default::default(),
        }
    }
}

impl PipelineBuilder<VSPipelineBuilder> {
    #[inline]
    pub fn with_fs_source(
        self,
        fs_source: StrongAssetId<WGSLSource>,
    ) -> PipelineBuilder<CompletePipelineBuilder> {
        PipelineBuilder {
            vs_source: self.vs_source,
            fs_source: Some(fs_source),
            color_blend: self.color_blend,
            alpha_blend: self.alpha_blend,
            priority: self.priority,
            _pd: Default::default(),
        }
    }
}

impl PipelineBuilder<CompletePipelineBuilder> {
    #[inline]
    pub fn finalize(self) -> Pipeline {
        Pipeline {
            vs_source: self.vs_source.unwrap(),
            fs_source: self.fs_source.unwrap(),
            color_blend: self.color_blend,
            alpha_blend: self.alpha_blend,
            priority: self.priority,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    pub vs_source: StrongAssetId<WGSLSource>,
    pub fs_source: StrongAssetId<WGSLSource>,
    #[serde(default)]
    pub color_blend: wgpu::BlendComponent,
    #[serde(default)]
    pub alpha_blend: wgpu::BlendComponent,
    #[serde(default)]
    pub priority: usize,
}

impl Pipeline {
    // TODO: wrong alpha order for text?
    pub const UNLIT_PRIORITY: usize = 0;
    pub const ALPHA_PRIORITY: usize = 1;
    pub const TEXT_PRIORITY: usize = 2;

    #[rustfmt::skip]
    const UNLIT_SHADER_UUID: Uuid = Uuid::from_bytes([
        0xb9, 0x5e, 0x09, 0x3d, 0xfa, 0x05, 0x4a, 0x24,
        0xa5, 0x92, 0x69, 0xbc, 0x01, 0x04, 0xba, 0x62
    ]);
    #[rustfmt::skip]
    const UNLIT_PIPELINE_UUID: Uuid = Uuid::from_bytes([
        0x0c, 0x7c, 0x1b, 0x6a, 0x96, 0xc7, 0x41, 0x6f,
        0x81, 0xbc, 0xf0, 0xf8, 0x6c, 0xba, 0x73, 0xc3
    ]);

    #[rustfmt::skip]
    const ALPHA_SHADER_UUID: Uuid = Uuid::from_bytes([
        0x08, 0xed, 0xbe, 0x55, 0x81, 0x44, 0x4d, 0x08,
        0x8b, 0xba, 0xae, 0xe5, 0xa7, 0xb6, 0x18, 0xca
    ]);
    #[rustfmt::skip]
    const ALPHA_PIPELINE_UUID: Uuid = Uuid::from_bytes([
        0x11, 0x24, 0xc4, 0x8f, 0x24, 0x18, 0x49, 0x4d,
        0xb5, 0x2c, 0xb7, 0x95, 0x62, 0x7d, 0xdb, 0xad
    ]);

    #[rustfmt::skip]
    const TEXT_SHADER_UUID: Uuid = Uuid::from_bytes([
        0xfc, 0xce, 0x60, 0xa6, 0xfc, 0x0d, 0x40, 0x24,
        0xae, 0x32, 0x12, 0x04, 0x43, 0x5c, 0xcc, 0xb0
    ]);
    #[rustfmt::skip]
    const TEXT_PIPELINE_UUID: Uuid = Uuid::from_bytes([
        0xee, 0xcf, 0xdc, 0x31, 0xf7, 0x17, 0x44, 0xe9,
        0x9d, 0x85, 0x89, 0x97, 0xfe, 0x4e, 0x26, 0x46
    ]);

    #[inline]
    pub fn builder() -> PipelineBuilder<EmptyPipelineBuilder> {
        PipelineBuilder {
            vs_source: None,
            fs_source: None,
            color_blend: wgpu::BlendComponent::REPLACE,
            alpha_blend: wgpu::BlendComponent::REPLACE,
            priority: 0,
            _pd: Default::default(),
        }
    }
}

pub struct RealizedPipeline {
    pub(crate) render_pipeline: wgpu::RenderPipeline,
    pub(crate) pipeline: Pipeline,
}

#[derive(Debug)]
pub struct WGSLSource(pub Cow<'static, str>);

pub struct WGSLSourceLoader;

impl AssetLoader for WGSLSourceLoader {
    type Asset = WGSLSource;

    #[inline]
    fn load<'a>(&self, cursor: &mut AssetCursor<'a>) -> anyhow::Result<Self::Asset> {
        let bytes = cursor.read()?;
        let shader_src = String::from_utf8(bytes)?;
        Ok(WGSLSource(shader_src.into()))
    }
}

pub struct Pipelines {
    shaders: HashMap<WeakAssetId<WGSLSource>, wgpu::ShaderModule>,
    loaded: HashMap<WeakAssetId<Pipeline>, RealizedPipeline>,
    queued: HashMap<WeakAssetId<Pipeline>, Vec<WeakAssetId<WGSLSource>>>,
    shader_index: BTreeSet<(WeakAssetId<WGSLSource>, OrderWindow<WeakAssetId<Pipeline>>)>,
    render_pipeline_layout: wgpu::PipelineLayout,
    samples: Samples,
    // defaults
    pub(crate) unlit_pipeline: StrongAssetId<Pipeline>,
    pub(crate) unlit_alpha_pipeline: StrongAssetId<Pipeline>,
    pub(crate) text_pipeline: StrongAssetId<Pipeline>,
}

impl Pipelines {
    pub fn new(
        assets: &AssetsClient,
        render_pipeline_layout: wgpu::PipelineLayout,
        samples: Samples,
    ) -> Self {
        let unlit_source = assets.store(
            Pipeline::UNLIT_SHADER_UUID,
            WGSLSource(include_str!("../../asset/shader/unlit.wgsl").into()),
        );
        let unlit_pipeline = assets.store(
            Pipeline::UNLIT_PIPELINE_UUID,
            Pipeline::builder()
                .with_vs_source(unlit_source.clone())
                .with_fs_source(unlit_source)
                .with_priority(Pipeline::UNLIT_PRIORITY)
                .finalize(),
        );

        let unlit_alpha_source = assets.store(
            Pipeline::ALPHA_SHADER_UUID,
            WGSLSource(include_str!("../../asset/shader/unlit_alpha.wgsl").into()),
        );
        let unlit_alpha_pipeline = assets.store(
            Pipeline::ALPHA_PIPELINE_UUID,
            Pipeline::builder()
                .with_vs_source(unlit_alpha_source.clone())
                .with_fs_source(unlit_alpha_source)
                .with_color_blend(wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                })
                .with_alpha_blend(wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                })
                .with_priority(Pipeline::ALPHA_PRIORITY)
                .finalize(),
        );

        let text_source = assets.store(
            Pipeline::TEXT_SHADER_UUID,
            WGSLSource(include_str!("../../asset/shader/text.wgsl").into()),
        );
        let text_pipeline = assets.store(
            Pipeline::TEXT_PIPELINE_UUID,
            Pipeline::builder()
                .with_vs_source(text_source.clone())
                .with_fs_source(text_source)
                .with_color_blend(wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                })
                .with_alpha_blend(wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                })
                .with_priority(Pipeline::TEXT_PRIORITY)
                .finalize(),
        );

        Self {
            shaders: Default::default(),
            loaded: Default::default(),
            queued: Default::default(),
            shader_index: Default::default(),
            render_pipeline_layout,
            samples,
            unlit_pipeline,
            unlit_alpha_pipeline,
            text_pipeline,
        }
    }

    pub fn pipelines_for_shader(
        &self,
        shader_id: WeakAssetId<WGSLSource>,
    ) -> Vec<WeakAssetId<Pipeline>> {
        use std::ops::Bound::Included;

        self.shader_index
            .range((
                Included(&(shader_id, OrderWindow::Start)),
                Included(&(shader_id, OrderWindow::End)),
            ))
            .filter_map(|(_, s)| s.as_option().copied())
            .collect()
    }

    pub fn upsert_shader(
        &mut self,
        device: &wgpu::Device,
        id: WeakAssetId<WGSLSource>,
        source: &WGSLSource,
    ) {
        log::debug!("upsert shader: {:?}", id);

        let module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("module"),
            flags: wgpu::ShaderFlags::all(),
            source: wgpu::ShaderSource::Wgsl(source.0.clone()),
        });

        if self.shaders.insert(id, module).is_some() {
            for (p_id, pipeline) in self
                .loaded
                .iter()
                .filter_map(|(p_id, p)| {
                    (p.pipeline.vs_source.is_same_asset(&id)
                        || p.pipeline.fs_source.is_same_asset(&id))
                    .then(|| (*p_id, p.pipeline.clone()))
                })
                .collect::<Vec<_>>()
            {
                assert!(
                    self.upsert_pipeline(device, p_id, &pipeline),
                    "upsert of pipeline on upsert shader"
                );
            }
        }
    }

    pub fn remove_shader(&mut self, id: &WeakAssetId<WGSLSource>) {
        log::debug!("remove shader: {:?}", id);
        self.shaders.remove(id);
    }

    pub fn queue_pipeline(&mut self, pipeline_id: WeakAssetId<Pipeline>, pipeline: &Pipeline) {
        self.remove_queued_pipeline(pipeline_id);
        self.queued.insert(
            pipeline_id,
            vec![pipeline.vs_source.to_weak(), pipeline.fs_source.to_weak()],
        );
        self.shader_index
            .insert((pipeline.vs_source.to_weak(), OrderWindow::new(pipeline_id)));
        self.shader_index
            .insert((pipeline.fs_source.to_weak(), OrderWindow::new(pipeline_id)));
    }

    pub fn get_pipeline(&self, pipeline_id: &WeakAssetId<Pipeline>) -> Option<&RealizedPipeline> {
        self.loaded.get(pipeline_id)
    }

    pub fn upsert_pipeline(
        &mut self,
        device: &wgpu::Device,
        pipeline_id: WeakAssetId<Pipeline>,
        pipeline: &Pipeline,
    ) -> bool {
        log::debug!("upsert pipeline: {:?}", pipeline_id);
        self.remove_queued_pipeline(pipeline_id);

        let vs_module = some_or_return!(self.shaders.get(&pipeline.vs_source.to_weak()), || false);
        let fs_module = some_or_return!(self.shaders.get(&pipeline.fs_source.to_weak()), || false);

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&self.render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: "main",
                buffers: &[Vertex::desc(), Instance::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: RealizedView::FRAME_TEXTURE_FORMAT,
                    write_mask: wgpu::ColorWrite::ALL,
                    blend: Some(wgpu::BlendState {
                        color: pipeline.color_blend,
                        alpha: pipeline.alpha_blend,
                    }),
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                clamp_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                // TODO: enable?
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: RealizedView::DEPTH_TEXTURE_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: self.samples.into(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

        self.remove_loaded_pipeline(pipeline_id);

        let raw = RealizedPipeline {
            render_pipeline,
            pipeline: pipeline.to_owned(),
        };

        self.loaded.insert(pipeline_id, raw);
        self.shader_index
            .insert((pipeline.vs_source.to_weak(), OrderWindow::new(pipeline_id)));
        self.shader_index
            .insert((pipeline.fs_source.to_weak(), OrderWindow::new(pipeline_id)));

        true
    }

    pub fn remove_pipeline(&mut self, pipeline_id: WeakAssetId<Pipeline>) {
        log::debug!("remove pipeline: {:?}", pipeline_id);
        self.remove_queued_pipeline(pipeline_id);
        self.remove_loaded_pipeline(pipeline_id);
    }

    fn remove_queued_pipeline(&mut self, pipeline_id: WeakAssetId<Pipeline>) {
        if let Some(sources) = self.queued.remove(&pipeline_id) {
            for source in sources {
                self.shader_index
                    .remove(&(source, OrderWindow::new(pipeline_id)));
            }
        }
    }

    fn remove_loaded_pipeline(&mut self, pipeline_id: WeakAssetId<Pipeline>) {
        if let Some(realized) = self.loaded.remove(&pipeline_id) {
            self.shader_index.remove(&(
                realized.pipeline.vs_source.to_weak(),
                OrderWindow::new(pipeline_id),
            ));
            self.shader_index.remove(&(
                realized.pipeline.fs_source.to_weak(),
                OrderWindow::new(pipeline_id),
            ));
        }
    }
}
