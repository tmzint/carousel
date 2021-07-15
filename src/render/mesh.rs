use crate::asset::loader::AssetLoader;
use crate::asset::storage::{Assets, AssetsClient};
use crate::asset::{StrongAssetId, WeakAssetId};
use crate::render::buffer::Vertex;
use crate::util::HashMap;
use relative_path::RelativePath;
use serde::Deserialize;
use uuid::Uuid;
use wgpu::util::DeviceExt;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    #[rustfmt::skip]
    const EMPTY_MESH_UUID: Uuid = Uuid::from_bytes([
        0xB2, 0xD2, 0x65, 0xE3, 0xA8, 0xF6, 0x4C, 0x62,
        0xB5, 0x03, 0x03, 0xEB, 0x2F, 0x11, 0xB6, 0x96,
    ]);

    #[rustfmt::skip]
    const UNIT_SQUARE_MESH_UUID: Uuid = Uuid::from_bytes([
        0x67, 0x65, 0xFF, 0xAD, 0x77, 0x81, 0x4B, 0xFF,
        0xBC, 0xE2, 0x17, 0x18, 0x12, 0xBB, 0xE2, 0x43,
    ]);
}

pub struct MeshLoader;

impl AssetLoader for MeshLoader {
    type Asset = Mesh;

    #[inline]
    fn deserialize<'a>(
        &self,
        path: &'a RelativePath,
        bytes: Vec<u8>,
        _assets: &'a Assets,
    ) -> anyhow::Result<Self::Asset> {
        let extension = path.extension().ok_or_else(|| {
            anyhow::anyhow!(
                "could not derive file type for serde asset loader: {}",
                path
            )
        })?;

        let asset = match extension {
            "json" => serde_json::from_slice(&bytes)?,
            s => Err(anyhow::anyhow!(
                "unhandled file type for mesh asset loader: {}",
                s
            ))?,
        };

        Ok(asset)
    }
}

pub struct RealizedMesh {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    pub(crate) index_length: u32,
}

impl RealizedMesh {
    pub fn new(device: &wgpu::Device, m: &Mesh) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&m.vertices),
            usage: wgpu::BufferUsage::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&m.indices),
            usage: wgpu::BufferUsage::INDEX,
        });

        RealizedMesh {
            vertex_buffer,
            index_buffer,
            index_length: m.indices.len() as u32,
        }
    }
}

pub struct Meshes {
    loaded: HashMap<WeakAssetId<Mesh>, RealizedMesh>,
    // defaults
    pub(crate) empty_mesh: StrongAssetId<Mesh>,
    pub(crate) unit_square_mesh: StrongAssetId<Mesh>,
}

impl Meshes {
    pub fn new(assets: &AssetsClient) -> Self {
        let empty_mesh = assets.store(
            Mesh::EMPTY_MESH_UUID,
            Mesh {
                vertices: Default::default(),
                indices: Default::default(),
            },
        );

        let unit_square_mesh = assets.store(
            Mesh::UNIT_SQUARE_MESH_UUID,
            Mesh {
                vertices: vec![
                    Vertex {
                        position: [-0.5, 0.5, 0.0],
                        tex_coords: [0.0, 0.0],
                    },
                    Vertex {
                        position: [0.5, 0.5, 0.0],
                        tex_coords: [1.0, 0.0],
                    },
                    Vertex {
                        position: [0.5, -0.5, 0.0],
                        tex_coords: [1.0, 1.0],
                    },
                    Vertex {
                        position: [-0.5, -0.5, 0.0],
                        tex_coords: [0.0, 1.0],
                    },
                ],
                indices: vec![1, 0, 3, 3, 2, 1],
            },
        );

        Self {
            loaded: Default::default(),
            empty_mesh,
            unit_square_mesh,
        }
    }

    pub fn get_mesh(&self, mesh_id: &WeakAssetId<Mesh>) -> Option<&RealizedMesh> {
        self.loaded.get(mesh_id)
    }

    pub fn upsert_mesh(&mut self, device: &wgpu::Device, mesh_id: WeakAssetId<Mesh>, mesh: &Mesh) {
        log::debug!("upsert mesh: {:?}", mesh_id);
        let realized = RealizedMesh::new(device, mesh);
        self.loaded.insert(mesh_id, realized);
    }

    pub fn remove_mesh(&mut self, mesh_id: &WeakAssetId<Mesh>) {
        log::debug!("remove mesh: {:?}", mesh_id);
        self.loaded.remove(mesh_id);
    }
}
