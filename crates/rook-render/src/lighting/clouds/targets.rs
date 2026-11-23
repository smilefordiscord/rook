//! What the march and the blend draw into, at a quarter of the frame's
//! pixels (half its width and height, as Studio's composite reads its
//! clouds), and the two sets of blended clouds they take turns writing:
//! each frame's blend reads the newest and writes the other.

use super::pipelines::{COLOR_FORMAT, DIST_FORMAT, HISTORY_DIST_FORMAT, target};
use super::*;

#[derive(Debug)]
pub(super) struct Targets {
    /// Half the frame's width and height, rounded up.
    pub(super) size: [u32; 2],
    pub(super) march_color: wgpu::TextureView,
    pub(super) march_dist: wgpu::TextureView,
    /// The blended clouds' light and their (distance, frames held).
    pub(super) history: [[wgpu::TextureView; 2]; 2],
    /// Which of `history` was written last.
    pub(super) newest: usize,
    /// The blend's group reading the march and `history[i]`.
    pub(super) resolve: [wgpu::BindGroup; 2],
    /// Drawing over the sky's group reading `history[i]`.
    pub(super) over: [wgpu::BindGroup; 2],
}

impl Targets {
    pub(super) fn new(
        device: &wgpu::Device,
        pipelines: &CloudPipelines,
        params: &wgpu::Buffer,
        size: [u32; 2],
    ) -> Self {
        let march_color = target(device, "rook clouds march", size, COLOR_FORMAT);
        let march_dist = target(device, "rook clouds distance", size, DIST_FORMAT);
        let history = [0, 1].map(|_| {
            [
                target(device, "rook clouds", size, COLOR_FORMAT),
                target(device, "rook clouds distance", size, HISTORY_DIST_FORMAT),
            ]
        });
        let view = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        let params = wgpu::BindGroupEntry {
            binding: 0,
            resource: params.as_entire_binding(),
        };
        let clamp = wgpu::BindGroupEntry {
            binding: 10,
            resource: wgpu::BindingResource::Sampler(&pipelines.clamp),
        };
        let resolve = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rook clouds blend"),
                layout: &pipelines.resolve_layout,
                entries: &[
                    params.clone(),
                    clamp.clone(),
                    view(11, &march_color),
                    view(12, &march_dist),
                    view(13, &history[i][0]),
                    view(14, &history[i][1]),
                ],
            })
        });
        let over = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rook clouds over"),
                layout: &pipelines.over_layout,
                entries: &[params.clone(), clamp.clone(), view(15, &history[i][0])],
            })
        });
        Self {
            size,
            march_color,
            march_dist,
            history,
            newest: 0,
            resolve,
            over,
        }
    }
}
