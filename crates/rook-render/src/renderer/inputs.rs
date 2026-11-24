//! What the renderer draws besides the scene: the GUI, the cursor, where
//! world GUI canvases are, the time, and where in the target the 3D view
//! goes.

use super::*;

impl Renderer {
    /// Draw `screen`'s pieces over the scene from now on, bottom first, and
    /// the world GUI's canvases as `world` says (`None` keeps what's
    /// drawn); their images go up as others do (quads draw nothing until
    /// theirs is up). Pieces and canvases given last time (the same `Arc`)
    /// stay on the GPU as they are.
    pub fn set_gui(
        &mut self,
        screen: Option<&Arc<[Arc<[GuiQuad]>]>>,
        world: Option<&WorldCanvases>,
    ) {
        fn claim(quads: &[GuiQuad], cache: &mut GpuCache<ImageHash, Image>) {
            for q in quads {
                match &q.paint {
                    rook_types::Paint::Image(paint) => {
                        cache.claim(ImageHash(paint.hash));
                    }
                    rook_types::Paint::Group(group) => claim(&group.quads, cache),
                    _ => {}
                }
            }
        }
        for fresh in self.gui.set(&self.device, &self.queue, screen, world) {
            claim(fresh.quads(), &mut self.images);
        }
    }

    /// Draw `cursor` over everything from now on, its image going up as
    /// others do, or none (`None`): Roblox draws its cursors into the
    /// frame (docs/behavior/gui/input.md#the-cursor).
    pub fn set_cursor(&mut self, cursor: Option<&GuiQuad>) {
        if let Some(quad) = cursor
            && let rook_types::Paint::Image(paint) = &quad.paint
        {
            self.images.claim(ImageHash(paint.hash));
        }
        (self.gui).set_cursor(&self.device, &self.queue, cursor);
    }

    /// Where the canvases at these indices into the last
    /// [`Renderer::set_gui`]'s `world` are now (`None`: not drawn). The
    /// others stay where they were; a new `world` starts with none placed.
    pub fn place_world_gui(
        &mut self,
        places: impl IntoIterator<Item = (usize, Option<WorldPlace>)>,
    ) {
        self.gui.place(places);
    }

    /// Set the clock grass sways by and Beams' images scroll by, in
    /// seconds; water and Trails move on by the time since the last call
    /// (none when the clock goes back, at most a second), water at the
    /// scene's `WaterWaveSpeed`. Particles move on when the next frame is
    /// drawn, only those it sees.
    pub fn set_time(&mut self, seconds: f32) {
        let dt = (seconds - self.time).clamp(0.0, 1.0);
        self.particles.sim.advance(dt);
        self.ribbons.sim.step(dt);
        self.water_phase =
            (self.water_phase + f64::from(dt * self.terrain.wave_speed)) % WATER_PHASE_PERIOD;
        self.clouds.advance(dt, self.terrain.wind);
        self.time = seconds;
    }

    /// Draw clouds with `noise` (the install's cloud textures) from now on,
    /// in place of Rook's own. False, keeping what it had, if they aren't
    /// laid out as the cloud shaders read them.
    pub fn set_cloud_noise(&mut self, noise: &crate::CloudNoise) -> bool {
        self.clouds.set_noise(&self.device, &self.queue, noise)
    }

    /// Draw the 3D view into `view` of the target from now on, projected
    /// for that rectangle's shape, and the GUI over the whole target; `None`
    /// fills the target, as a game's window does. An editor draws its view
    /// between its panels this way, so the view's center is the middle of
    /// what shows. Outside the rectangle is black under the GUI.
    pub fn set_view(&mut self, view: Option<ViewRect>) {
        self.view = view;
    }

    /// Where the 3D view goes in a `width` × `height` target.
    pub(crate) fn view_in(&self, width: u32, height: u32) -> ViewRect {
        let whole = ViewRect {
            x: 0,
            y: 0,
            width,
            height,
        };
        self.view
            .unwrap_or(whole)
            .within(width.max(1), height.max(1))
    }
}
