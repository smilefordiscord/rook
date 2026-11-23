//! The post effects' passes between the main pass and the tone pass
//! (docs/behavior/lighting/post-processing.md). Each enabled effect draws
//! its own textures, at a reduced resolution, from the frame; the tone
//! pass, Studio's last pass, reads them through [`Post::tone_group`] and
//! composes them with the frame. With no effect enabled nothing here
//! runs or is allocated: the tone pass reads a black stand-in texel.
//! Textures are made the first frame an effect needs them and again when
//! the target's size changes.

pub(crate) mod bloom;
pub(crate) mod blur;
pub(crate) mod sun_rays;

use wesl_core::include_wesl;

use super::*;
use crate::renderer::passes::tonemap::FRAME_FORMAT;

/// The bytes each pass's parameters take in [`Slots`]: wgpu's least
/// uniform offset alignment.
const SLOT: u64 = 256;

/// How many passes a frame's effects may run.
const SLOTS: u64 = 32;

/// Every pass's parameters for a frame, written to the GPU once before
/// the frame's passes run, each pass reading its own slot by a dynamic
/// offset.
#[derive(Debug)]
struct Slots {
    buffer: wgpu::Buffer,
    bytes: Vec<u8>,
}

impl Slots {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook post parameters"),
                size: SLOT * SLOTS,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            bytes: Vec::new(),
        }
    }

    /// A slot holding `value`; its offset.
    fn push(&mut self, value: &impl Pod) -> u32 {
        let at = self.bytes.len();
        assert!(
            (at as u64) < SLOT * SLOTS,
            "a frame's post passes outgrew their slots"
        );
        self.bytes.resize(at + SLOT as usize, 0);
        let value = bytemuck::bytes_of(value);
        self.bytes[at..at + value.len()].copy_from_slice(value);
        at as u32
    }

    fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.bytes.is_empty() {
            queue.write_buffer(&self.buffer, 0, &self.bytes);
        }
        self.bytes.clear();
    }
}

/// What every post pass binds: its parameters, two textures and a
/// filtering sampler that clamps at the edges. The second texture is the
/// depth buffer for the passes that read depth.
#[derive(Debug)]
pub(crate) struct Shared {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    depth_layout: wgpu::BindGroupLayout,
    depth_pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    slots: Slots,
    /// One black texel, for a texture a pass or the tone pass doesn't use.
    black: wgpu::TextureView,
}

/// A texture a post pass draws into and others read.
#[derive(Debug)]
struct Target {
    view: wgpu::TextureView,
}

impl Shared {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let depth = wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layouts = |second: wgpu::BindGroupLayoutEntry| {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rook post"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(SLOT),
                        },
                        count: None,
                    },
                    texture(1),
                    second,
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
            let pipeline = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("rook post"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            (layout, pipeline)
        };
        let (layout, pipeline_layout) = layouts(texture(2));
        let (depth_layout, depth_pipeline_layout) = layouts(depth);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rook post"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let black = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("rook post black"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[0, 0, 0, 255],
        );
        Self {
            layout,
            pipeline_layout,
            depth_layout,
            depth_pipeline_layout,
            sampler,
            slots: Slots::new(device),
            black: black.create_view(&Default::default()),
        }
    }

    /// A full-screen pass's pipeline: `entry` of `module` drawing one
    /// triangle over the target, blended by `blend`.
    fn pipeline(
        &self,
        device: &wgpu::Device,
        module: &wgpu::ShaderModule,
        entry: &str,
        blend: Option<wgpu::BlendState>,
    ) -> wgpu::RenderPipeline {
        Self::make_pipeline(device, &self.pipeline_layout, (module, entry), blend)
    }

    /// [`Shared::pipeline`] for a pass reading the depth buffer.
    fn depth_pipeline(
        &self,
        device: &wgpu::Device,
        module: &wgpu::ShaderModule,
        entry: &str,
    ) -> wgpu::RenderPipeline {
        Self::make_pipeline(device, &self.depth_pipeline_layout, (module, entry), None)
    }

    fn make_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        (module, entry): (&wgpu::ShaderModule, &str),
        blend: Option<wgpu::BlendState>,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_post"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FRAME_FORMAT,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    /// A bind group reading `a` and `b` (black when `None`).
    fn group(
        &self,
        device: &wgpu::Device,
        a: &wgpu::TextureView,
        b: Option<&wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        self.make_group(device, &self.layout, a, b.unwrap_or(&self.black))
    }

    /// A bind group reading `a` and the depth buffer's `depth` aspect.
    fn depth_group(
        &self,
        device: &wgpu::Device,
        a: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.make_group(device, &self.depth_layout, a, depth)
    }

    fn make_group(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        a: &wgpu::TextureView,
        b: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook post"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.slots.buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(SLOT),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(a),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(b),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// A `width` × `height` texture in the frame's format.
    fn target(device: &wgpu::Device, label: &str, (width, height): (u32, u32)) -> Target {
        let size = (width.max(1), height.max(1));
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FRAME_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Target { view }
    }

    /// Draw `pipeline` over all of `target` from `group`, its parameters
    /// at `slot`; cleared first unless `keep`, when the pass blends onto
    /// what's there.
    fn run(
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
        target: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        group: &wgpu::BindGroup,
        slot: u32,
        keep: bool,
    ) {
        let load = if keep {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[slot]);
        pass.draw(0..3, 0..1);
    }
}

/// What the post passes read of the frame being drawn.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PostView<'a> {
    /// The frame, in its encoding (`shaders/tone.wgsl`).
    pub(crate) frame: &'a wgpu::TextureView,
    /// The main pass's depth buffer, its depth alone.
    pub(crate) depth: &'a wgpu::TextureView,
    /// The frame's size in pixels.
    pub(crate) size: (u32, u32),
    pub(crate) camera: &'a Camera,
    pub(crate) lighting: &'a Lighting,
}

/// What the tone pass reads besides the frame: each effect's texture,
/// and how much of each it takes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ToneInputs {
    /// How much of the blurred frame replaces the frame, 0 to 1.
    pub(crate) blur: f32,
    /// 1 when the bloom texture holds light to add.
    pub(crate) bloom: f32,
    /// 1 when the sun rays texture holds light to add.
    pub(crate) rays: f32,
    /// The Blooms without bleed (`Size` 0), added in place: each one's
    /// threshold in light units, and its intensity (0 for none).
    pub(crate) in_place: [[f32; 4]; 2],
}

/// The textures the tone pass reads, `None` where it reads black.
#[derive(Debug, Clone, Default, PartialEq)]
struct ToneReads {
    blurred: Option<wgpu::TextureView>,
    bloom: Option<wgpu::TextureView>,
    rays: Option<wgpu::TextureView>,
}

/// How many textures the tone pass's group 1 holds after its sampler.
const TONE_TEXTURES: u32 = 3;

/// The post effects' pipelines and textures.
#[derive(Debug)]
pub(crate) struct Post {
    shared: Shared,
    /// The tone pass's group 1 layout: the sampler and each effect's
    /// texture.
    tone_layout: wgpu::BindGroupLayout,
    /// The tone pass's group 1, and the textures it was made from.
    tone_group: wgpu::BindGroup,
    tone_reads: ToneReads,
    blur: blur::Blur,
    bloom: bloom::BloomPasses,
    sun_rays: sun_rays::SunRaysPasses,
}

impl Post {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shared = Shared::new(device, queue);
        let mut entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        }];
        entries.extend(
            (1..=TONE_TEXTURES).map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }),
        );
        let tone_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook tone post"),
            entries: &entries,
        });
        let module = |label, source: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            })
        };
        let blur = blur::Blur::new(device, &shared, &module("rook blur", include_wesl!("blur")));
        let bloom = bloom::BloomPasses::new(
            device,
            &shared,
            &module("rook bloom", include_wesl!("bloom")),
        );
        let sun_rays = sun_rays::SunRaysPasses::new(
            device,
            &shared,
            &module("rook sun rays", include_wesl!("sun_rays")),
        );
        let tone_reads = ToneReads::default();
        let tone_group = Self::make_tone_group(device, &shared, &tone_layout, &tone_reads);
        Self {
            shared,
            tone_layout,
            tone_group,
            tone_reads,
            blur,
            bloom,
            sun_rays,
        }
    }

    pub(crate) fn tone_layout(&self) -> &wgpu::BindGroupLayout {
        &self.tone_layout
    }

    pub(crate) fn tone_group(&self) -> &wgpu::BindGroup {
        &self.tone_group
    }

    fn make_tone_group(
        device: &wgpu::Device,
        shared: &Shared,
        layout: &wgpu::BindGroupLayout,
        reads: &ToneReads,
    ) -> wgpu::BindGroup {
        fn texture<'a>(
            binding: u32,
            view: Option<&'a wgpu::TextureView>,
            black: &'a wgpu::TextureView,
        ) -> wgpu::BindGroupEntry<'a> {
            wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(view.unwrap_or(black)),
            }
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook tone post"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&shared.sampler),
                },
                texture(1, reads.blurred.as_ref(), &shared.black),
                texture(2, reads.bloom.as_ref(), &shared.black),
                texture(3, reads.rays.as_ref(), &shared.black),
            ],
        })
    }

    /// Run the enabled effects' passes from `view` and point the tone
    /// group at what they drew; what the tone pass takes of each.
    pub(crate) fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &PostView,
        effects: &PostEffects,
    ) -> ToneInputs {
        let (frame, size) = (view.frame, view.size);
        let shared = &mut self.shared;
        let rays = (self.sun_rays).record(device, shared, encoder, view, effects.sun_rays);
        let bloom = (self.bloom).record(device, shared, encoder, (frame, size), &effects.bloom);
        let blurred = (self.blur).record(device, shared, encoder, frame, size, effects.blur);
        shared.slots.upload(queue);
        let reads = ToneReads {
            blurred,
            bloom: bloom.texture,
            rays,
        };
        let inputs = ToneInputs {
            blur: if reads.blurred.is_some() { 1.0 } else { 0.0 },
            bloom: if reads.bloom.is_some() { 1.0 } else { 0.0 },
            rays: if reads.rays.is_some() { 1.0 } else { 0.0 },
            in_place: bloom.in_place,
        };
        if reads != self.tone_reads {
            self.tone_group =
                Self::make_tone_group(device, &self.shared, &self.tone_layout, &reads);
            self.tone_reads = reads;
        }
        inputs
    }
}
