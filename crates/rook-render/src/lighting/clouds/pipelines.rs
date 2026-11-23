//! The cloud passes' pipelines and the bind groups they read: the cloud
//! textures (group 2 of the shadow map and the march), the shadow map
//! (group 3 of the march), the march and last frame's clouds (group 2 of
//! the blend) and this frame's (group 2 of drawing them over the sky).
//! Groups 0 and 1 are the globals and the main group, as for every pass.

use wesl_core::include_wesl;

use super::*;

/// The shadow map's texels along each side, over one tile of the field
/// (Approximation: Studio's size isn't in the pack).
const SHADOW_MAP_SIDE: u32 = 128;

/// The march's light (in light units) and how much it covers.
pub(super) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The march's distance to its first cloud (km): exact, as the blend tells
/// moved clouds from still ones to a thousandth of it.
pub(super) const DIST_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
/// The blended clouds' distance and how many frames they hold.
pub(super) const HISTORY_DIST_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Float;

#[derive(Debug)]
pub(crate) struct CloudPipelines {
    pub(super) shadow: wgpu::RenderPipeline,
    pub(super) march: wgpu::RenderPipeline,
    pub(super) resolve: wgpu::RenderPipeline,
    pub(crate) over: wgpu::RenderPipeline,
    shapes_layout: wgpu::BindGroupLayout,
    shadow_layout: wgpu::BindGroupLayout,
    pub(super) resolve_layout: wgpu::BindGroupLayout,
    pub(super) over_layout: wgpu::BindGroupLayout,
    /// Repeating every way: the detail, the flow and the shadow map.
    repeat: wgpu::Sampler,
    /// Repeating across, clamped up and down: the field.
    layer: wgpu::Sampler,
    /// Clamped: the march and the blended clouds.
    pub(super) clamp: wgpu::Sampler,
}

const FRAGMENT: wgpu::ShaderStages = wgpu::ShaderStages::FRAGMENT;

fn texture(
    binding: u32,
    dimension: wgpu::TextureViewDimension,
    filterable: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: dimension,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

/// The cloud uniform, binding 0 of every group 2.
const PARAMS: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
    binding: 0,
    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
    ty: wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    },
    count: None,
};

impl CloudPipelines {
    /// The pipelines, after the globals' and the main group's `layouts`,
    /// drawing over the sky into a `frame_format` frame.
    pub(super) fn new(
        device: &wgpu::Device,
        layouts: [&wgpu::BindGroupLayout; 2],
        frame_format: wgpu::TextureFormat,
    ) -> Self {
        use wgpu::TextureViewDimension::{D2, D3};
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let shapes_layout = layout(
            "rook cloud shapes",
            &[
                PARAMS,
                sampler(1),
                sampler(2),
                texture(3, D3, true),
                texture(4, D3, true),
                texture(5, D2, true),
            ],
        );
        let shadow_layout = layout("rook cloud shadows", &[texture(0, D2, true)]);
        let resolve_layout = layout(
            "rook cloud blend",
            &[
                PARAMS,
                sampler(10),
                texture(11, D2, true),
                texture(12, D2, false),
                texture(13, D2, true),
                texture(14, D2, false),
            ],
        );
        let over_layout = layout(
            "rook clouds over",
            &[PARAMS, sampler(10), texture(15, D2, true)],
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook clouds"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("clouds").into()),
        });
        let pipeline = |label,
                        own: &[&wgpu::BindGroupLayout],
                        fragment,
                        targets: &[Option<wgpu::ColorTargetState>],
                        depth: Option<wgpu::DepthStencilState>| {
            let groups: Vec<_> = layouts.iter().chain(own).map(|l| Some(*l)).collect();
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &groups,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_clouds"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: depth,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let plain = |format: wgpu::TextureFormat| Some(format.into());
        let shadow = pipeline(
            "rook cloud shadows",
            &[&shapes_layout],
            "fs_cloud_shadow",
            &[plain(COLOR_FORMAT)],
            None,
        );
        let march = pipeline(
            "rook clouds march",
            &[&shapes_layout, &shadow_layout],
            "fs_cloud_march",
            &[plain(COLOR_FORMAT), plain(DIST_FORMAT)],
            None,
        );
        let resolve = pipeline(
            "rook clouds blend",
            &[&resolve_layout],
            "fs_cloud_resolve",
            &[plain(COLOR_FORMAT), plain(HISTORY_DIST_FORMAT)],
            None,
        );
        // Over the sky in the frame's encoding, as Studio's composite draws
        // them (alpha blended; the frame's alpha kept), untested against
        // depth like the sky: the parts drawn after cover them.
        let over_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let over = pipeline(
            "rook clouds over",
            &[&over_layout],
            "fs_cloud_over",
            &[Some(wgpu::ColorTargetState {
                format: frame_format,
                blend: Some(over_blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            Some(wgpu::DepthStencilState {
                format: main_depth_format(device),
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
        );
        let make_sampler = |label, across, up| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: across,
                address_mode_v: across,
                address_mode_w: up,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        use wgpu::AddressMode::{ClampToEdge, Repeat};
        Self {
            shadow,
            march,
            resolve,
            over,
            shapes_layout,
            shadow_layout,
            resolve_layout,
            over_layout,
            repeat: make_sampler("rook clouds repeat", Repeat, Repeat),
            layer: make_sampler("rook clouds layer", Repeat, ClampToEdge),
            clamp: make_sampler("rook clouds clamp", ClampToEdge, ClampToEdge),
        }
    }

    /// `noise` on the GPU, bound with `params` for the shadow map and the
    /// march.
    pub(super) fn shapes_group(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &wgpu::Buffer,
        noise: &CloudNoise,
    ) -> wgpu::BindGroup {
        let upload = |label, volume: &Volume| {
            let format = match volume.channels {
                1 => wgpu::TextureFormat::R8Unorm,
                _ => wgpu::TextureFormat::Rg8Unorm,
            };
            let [width, height, depth] = volume.size;
            let dimension = if depth > 1 {
                wgpu::TextureDimension::D3
            } else {
                wgpu::TextureDimension::D2
            };
            let texture = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: depth,
                    },
                    mip_level_count: volume.levels.len() as u32,
                    sample_count: 1,
                    dimension,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &volume.levels.concat(),
            );
            texture.create_view(&Default::default())
        };
        let field = upload("rook cloud field", &noise.field);
        let detail = upload("rook cloud detail", &noise.detail);
        let flow = upload("rook cloud flow", &noise.flow);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook cloud shapes"),
            layout: &self.shapes_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.repeat),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.layer),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&field),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&detail),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&flow),
                },
            ],
        })
    }

    /// The shadow map the first pass draws, and its group for the march.
    pub(super) fn shadow_map(&self, device: &wgpu::Device) -> (wgpu::TextureView, wgpu::BindGroup) {
        let view = target(
            device,
            "rook cloud shadows",
            [SHADOW_MAP_SIDE; 2],
            COLOR_FORMAT,
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook cloud shadows"),
            layout: &self.shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        (view, group)
    }
}

/// A `size` texture of `format` a pass draws into and a later one reads.
pub(super) fn target(
    device: &wgpu::Device,
    label: &str,
    [width, height]: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
