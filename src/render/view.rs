use crate::asset::storage::{Assets, AssetsClient};
use crate::asset::{StrongAssetId, WeakAssetId};
use crate::prelude::AssetLoader;
use crate::util::{HashMap, OrderWindow};
use image::{DynamicImage, GenericImageView, ImageBuffer};
use relative_path::RelativePath;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::num::NonZeroU32;
use uuid::Uuid;

pub enum BufferKind {
    Frame,
    Depth,
    Image,
}

pub struct RealizedView {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub samples: u32,
    pub kind: BufferKind,
    pub size: wgpu::Extent3d,
}

impl RealizedView {
    // TODO: let preferred_format = adapter.get_swap_chain_preferred_format(&surface);
    pub const FRAME_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
    pub const IMAGE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
    pub const DEPTH_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn frame_buffer(
        device: &wgpu::Device,
        size: [u32; 2],
        samples: u32,
        label: Option<&str>,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FRAME_TEXTURE_FORMAT,
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
        };

        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        RealizedView {
            texture,
            view,
            samples,
            kind: BufferKind::Frame,
            size,
        }
    }

    pub fn depth_buffer(
        device: &wgpu::Device,
        size: [u32; 2],
        samples: u32,
        label: Option<&str>,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_TEXTURE_FORMAT,
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
        };
        let texture = device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        RealizedView {
            texture,
            view,
            samples,
            kind: BufferKind::Depth,
            size,
        }
    }

    pub fn image_texture_buffer(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &DynamicImage,
        atlas: [NonZeroU32; 2],
        label: Option<&str>,
    ) -> Self {
        let dimensions = image.dimensions();
        let width = dimensions.0 / atlas[0].get();
        let height = dimensions.1 / atlas[1].get();
        let layers = unsafe { NonZeroU32::new_unchecked(atlas[0].get() * atlas[1].get()) };
        let rgba = image.to_rgba8();

        let realized = Self::empty_image_texture_buffer(device, [width, height], layers, label);

        if layers.get() == 1 {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &realized.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                },
                rgba.as_raw(),
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: NonZeroU32::new(4 * width),
                    rows_per_image: NonZeroU32::new(height),
                },
                realized.size,
            );
        } else {
            for layer in 0..layers.get() {
                let x = layer % atlas[0].get();
                let y = layer / atlas[0].get();
                let view = rgba.view(x * width, y * height, width, height).to_image();
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &realized.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: layer,
                        },
                    },
                    view.as_raw(),
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: NonZeroU32::new(4 * width),
                        rows_per_image: NonZeroU32::new(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        realized
    }

    pub fn empty_image_texture_buffer(
        device: &wgpu::Device,
        size: [u32; 2],
        layers: NonZeroU32,
        label: Option<&str>,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: layers.get(),
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::IMAGE_TEXTURE_FORMAT,
            usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(layers),
            ..Default::default()
        });

        RealizedView {
            texture,
            view,
            samples: 1,
            kind: BufferKind::Image,
            size,
        }
    }
}

pub struct ImageLoader;

impl AssetLoader for ImageLoader {
    type Asset = DynamicImage;

    #[inline]
    fn deserialize<'a>(
        &self,
        _path: &'a RelativePath,
        bytes: Vec<u8>,
        _assets: &'a mut Assets,
    ) -> anyhow::Result<Self::Asset> {
        let img = image::load_from_memory(&bytes)?;
        Ok(img)
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Deserialize)]
pub enum FilterMode {
    Nearest = 0,
    Linear = 1,
}

impl Default for FilterMode {
    #[inline]
    fn default() -> Self {
        FilterMode::Linear
    }
}

impl Into<wgpu::FilterMode> for FilterMode {
    #[inline]
    fn into(self) -> wgpu::FilterMode {
        match self {
            FilterMode::Nearest => wgpu::FilterMode::Nearest,
            FilterMode::Linear => wgpu::FilterMode::Linear,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Texture {
    pub image: StrongAssetId<DynamicImage>,
    #[serde(default)]
    pub mag_filter: FilterMode,
    #[serde(default)]
    pub min_filter: FilterMode,
    #[serde(default = "non_zero_grid_one")]
    pub atlas: [NonZeroU32; 2],
}

impl Texture {
    #[rustfmt::skip]
    const WHITE_IMAGE_UUID: Uuid = Uuid::from_bytes([
        0x51, 0xc2, 0x71, 0x88, 0x8d, 0x91, 0x41, 0x41,
        0x82, 0xa2, 0xdc, 0xd7, 0xf4, 0x30, 0xdb, 0x6c
    ]);

    #[rustfmt::skip]
    const WHITE_TEXTURE_UUID: Uuid = Uuid::from_bytes([
        0x40, 0xc6, 0xc8, 0x0b, 0xe0, 0x19, 0x47, 0x37,
        0x9b, 0x88, 0xba, 0xec, 0x81, 0x54, 0x50, 0xc0
    ]);

    #[inline]
    pub fn new(image: StrongAssetId<DynamicImage>) -> Self {
        Self {
            image,
            mag_filter: FilterMode::default(),
            min_filter: FilterMode::default(),
            atlas: non_zero_grid_one(),
        }
    }

    #[inline]
    pub fn with_mag_filter(mut self, filter: FilterMode) -> Self {
        self.mag_filter = filter;
        self
    }

    #[inline]
    pub fn with_min_filter(mut self, filter: FilterMode) -> Self {
        self.min_filter = filter;
        self
    }

    #[inline]
    pub fn with_atlas(mut self, atlas: [NonZeroU32; 2]) -> Self {
        self.atlas = atlas;
        self
    }
}

pub struct RealizedTexture {
    pub(crate) view: RealizedView,
    pub(crate) sampler: wgpu::Sampler,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) image: WeakAssetId<DynamicImage>,
}

impl RealizedTexture {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &Texture,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        image: &DynamicImage,
    ) -> Self {
        let view = RealizedView::image_texture_buffer(
            device,
            queue,
            image,
            texture.atlas,
            Some("texture_buffer"),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: texture.mag_filter.into(),
            min_filter: texture.min_filter.into(),
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("texture_bind_group"),
        });

        Self {
            view,
            sampler,
            bind_group,
            image: texture.image.to_weak(),
        }
    }
}

pub struct Textures {
    loaded: HashMap<WeakAssetId<Texture>, RealizedTexture>,
    queued: HashMap<WeakAssetId<Texture>, WeakAssetId<DynamicImage>>,
    image_index: BTreeSet<(WeakAssetId<DynamicImage>, OrderWindow<WeakAssetId<Texture>>)>,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    // defaults
    pub(crate) white_texture: StrongAssetId<Texture>,
}

impl Textures {
    pub fn new(assets: &AssetsClient, texture_bind_group_layout: wgpu::BindGroupLayout) -> Self {
        let white_image = assets.store(
            Texture::WHITE_IMAGE_UUID,
            DynamicImage::ImageRgba8(ImageBuffer::from_fn(1, 1, |_x, _y| {
                image::Rgba([255u8, 255u8, 255u8, 255u8])
            })),
        );
        let white_texture = assets.store(
            Texture::WHITE_TEXTURE_UUID,
            Texture::new(white_image)
                .with_mag_filter(FilterMode::Nearest)
                .with_min_filter(FilterMode::Nearest),
        );

        Self {
            loaded: Default::default(),
            queued: Default::default(),
            image_index: Default::default(),
            texture_bind_group_layout,
            white_texture,
        }
    }

    pub fn textures_for_image(
        &self,
        image_id: WeakAssetId<DynamicImage>,
    ) -> Vec<WeakAssetId<Texture>> {
        use std::ops::Bound::Included;

        self.image_index
            .range((
                Included(&(image_id, OrderWindow::Start)),
                Included(&(image_id, OrderWindow::End)),
            ))
            .filter_map(|(_, t)| t.as_option().copied())
            .collect()
    }

    pub fn queue_texture(&mut self, texture_id: WeakAssetId<Texture>, texture: &Texture) {
        self.remove_queued_texture(texture_id);
        self.queued.insert(texture_id, texture.image.to_weak());
        self.image_index
            .insert((texture.image.to_weak(), OrderWindow::new(texture_id)));
    }

    pub fn get_texture(&self, texture_id: &WeakAssetId<Texture>) -> Option<&RealizedTexture> {
        self.loaded.get(texture_id)
    }

    pub fn upsert_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_id: WeakAssetId<Texture>,
        texture: &Texture,
        image: &DynamicImage,
    ) {
        log::debug!("upsert texture: {:?}", texture_id);
        self.remove_texture(texture_id);

        let realized = RealizedTexture::new(
            device,
            queue,
            texture,
            &self.texture_bind_group_layout,
            image,
        );

        self.loaded.insert(texture_id, realized);
        self.image_index
            .insert((texture.image.to_weak(), OrderWindow::new(texture_id)));
    }

    pub fn remove_texture(&mut self, texture_id: WeakAssetId<Texture>) {
        log::debug!("remove texture: {:?}", texture_id);
        self.remove_queued_texture(texture_id);
        self.remove_loaded_texture(texture_id);
    }

    fn remove_queued_texture(&mut self, texture_id: WeakAssetId<Texture>) {
        if let Some(image) = self.queued.remove(&texture_id) {
            self.image_index
                .remove(&(image, OrderWindow::new(texture_id)));
        }
    }

    fn remove_loaded_texture(&mut self, texture_id: WeakAssetId<Texture>) {
        if let Some(realized) = self.loaded.remove(&texture_id) {
            self.image_index
                .remove(&(realized.image, OrderWindow::new(texture_id)));
        }
    }
}

fn non_zero_grid_one() -> [NonZeroU32; 2] {
    let one = NonZeroU32::new(1).unwrap();
    [one, one]
}
