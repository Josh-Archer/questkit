//! Optional physical keyboard tracking and a projected passthrough patch.
//!
//! Own the raw FB mesh/geometry handles here because openxr has no safe wrappers for them.
//! The projected layer is independent of full-room reconstruction passthrough.
use std::ptr;
use std::time::{Duration, Instant};

use openxr::sys::Handle as _;
use openxr::{sys, Session, Space, Time, Vulkan};

use crate::spatial::keyboard::DEFAULT_KEYBOARD_MARGIN_M;
use crate::spatial::{Quat, ScreenPose, TrackedKeyboard, Vec3};

fn check(result: sys::Result) -> openxr::Result<()> {
    if result.into_raw() < 0 {
        Err(result)
    } else {
        Ok(())
    }
}

pub struct KeyboardTracking {
    dynamic: Option<super::dynamic_keyboard::DynamicKeyboard>,
    session: Session<Vulkan>,
    space: Option<Space>,
    description: Option<sys::KeyboardTrackingDescriptionFB>,
    next_query: Instant,
    geometry: sys::GeometryInstanceFB,
    mesh: sys::TriangleMeshFB,
    layer: openxr::PassthroughLayerFB,
    _passthrough: openxr::Passthrough,
    tracked: Option<TrackedKeyboard>,
}

impl KeyboardTracking {
    pub fn new(
        session: &Session<Vulkan>,
        dynamic: bool,
        system: sys::SystemId,
    ) -> openxr::Result<Self> {
        let dynamic = if dynamic {
            match super::dynamic_keyboard::DynamicKeyboard::new(session, system) {
                Ok(tracker) => Some(tracker),
                Err(error) if session.instance().exts().fb_keyboard_tracking.is_some() => {
                    tracing::warn!(?error, "generic keyboard unavailable; using legacy tracker");
                    None
                }
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        let instance = session.instance();
        let mesh_fp = instance.exts().fb_triangle_mesh.as_ref().unwrap();
        let passthrough =
            session.create_passthrough(openxr::PassthroughFlagsFB::IS_RUNNING_AT_CREATION)?;
        let layer = session.create_passthrough_layer(
            &passthrough,
            openxr::PassthroughFlagsFB::IS_RUNNING_AT_CREATION,
            openxr::PassthroughLayerPurposeFB::PROJECTED,
        )?;
        // A unit rectangle in the keyboard's XZ plane, facing up. Pose and size are applied
        // to its geometry instance each frame, without rebuilding or uploading a mesh.
        let vertices = [(-0.5, -0.5), (-0.5, 0.5), (0.5, 0.5), (0.5, -0.5)]
            .map(|(x, z)| sys::Vector3f { x, y: 0.0, z });
        let indices = [0, 1, 2, 0, 2, 3];
        let info = sys::TriangleMeshCreateInfoFB {
            ty: sys::TriangleMeshCreateInfoFB::TYPE,
            next: ptr::null(),
            flags: sys::TriangleMeshFlagsFB::EMPTY,
            winding_order: sys::WindingOrderFB::CCW,
            vertex_count: vertices.len() as u32,
            vertex_buffer: vertices.as_ptr(),
            triangle_count: 2,
            index_buffer: indices.as_ptr(),
        };
        let mut mesh = sys::TriangleMeshFB::NULL;
        // SAFETY: buffers live through the call; handles and function table share a session.
        unsafe {
            check((mesh_fp.create_triangle_mesh)(
                session.as_raw(),
                &info,
                &mut mesh,
            ))?;
        }
        Ok(Self {
            dynamic,
            session: session.clone(),
            space: None,
            description: None,
            next_query: Instant::now(),
            geometry: sys::GeometryInstanceFB::NULL,
            mesh,
            layer,
            _passthrough: passthrough,
            tracked: None,
        })
    }

    pub fn layer(&self) -> Option<&openxr::PassthroughLayerFB> {
        self.tracked.as_ref().map(|_| &self.layer)
    }

    /// Invalid or lost tracking never leaves the last known patch visible.
    pub fn update(&mut self, base: &Space, time: Time) -> Option<TrackedKeyboard> {
        let was_tracking = self.tracked.is_some();
        self.tracked = None;
        match self.update_inner(base, time) {
            Ok(tracked) => self.tracked = tracked,
            Err(error) => {
                // Retry discovery at most once per second, including runtimes which advertise
                // the extension but have no supported keyboard. Do not log per-frame failures.
                if self.space.take().is_some() {
                    tracing::warn!(?error, "keyboard tracking lost; hiding passthrough patch");
                }
                self.description = None;
            }
        }
        if was_tracking != self.tracked.is_some() {
            tracing::info!(
                tracked = self.tracked.is_some(),
                "keyboard passthrough tracking changed"
            );
        }
        self.tracked
    }

    fn update_inner(
        &mut self,
        base: &Space,
        time: Time,
    ) -> openxr::Result<Option<TrackedKeyboard>> {
        let keyboard = if let Some(dynamic) = &mut self.dynamic {
            dynamic.update(base, time)?
        } else {
            self.update_legacy(base, time)?
        };
        let Some(keyboard) = keyboard else {
            return Ok(None);
        };
        let passthrough_fp = self
            .session
            .instance()
            .exts()
            .fb_passthrough
            .as_ref()
            .unwrap();
        let pose = openxr::Posef::from(keyboard.pose);
        let scale = sys::Vector3f {
            x: keyboard.width_m + 2.0 * DEFAULT_KEYBOARD_MARGIN_M,
            y: 1.0,
            z: keyboard.depth_m + 2.0 * DEFAULT_KEYBOARD_MARGIN_M,
        };
        if self.geometry == sys::GeometryInstanceFB::NULL {
            let info = sys::GeometryInstanceCreateInfoFB {
                ty: sys::GeometryInstanceCreateInfoFB::TYPE,
                next: ptr::null(),
                layer: self.layer.as_raw(),
                mesh: self.mesh,
                base_space: base.as_raw(),
                pose,
                scale,
            };
            // SAFETY: the session owns all referenced handles; this object destroys geometry first.
            unsafe {
                check((passthrough_fp.create_geometry_instance)(
                    self.session.as_raw(),
                    &info,
                    &mut self.geometry,
                ))?;
            }
        } else {
            let transform = sys::GeometryInstanceTransformFB {
                ty: sys::GeometryInstanceTransformFB::TYPE,
                next: ptr::null(),
                base_space: base.as_raw(),
                time,
                pose,
                scale,
            };
            unsafe {
                check((passthrough_fp.geometry_instance_set_transform)(
                    self.geometry,
                    &transform,
                ))?;
            }
        }
        Ok(Some(keyboard))
    }
    pub fn event(&mut self, event: &sys::EventDataBuffer) {
        if let Some(dynamic) = &mut self.dynamic {
            if let Err(error) = dynamic.event(event) {
                tracing::warn!(?error, "generic keyboard event failed");
            }
        }
    }

    fn update_legacy(
        &mut self,
        base: &Space,
        time: Time,
    ) -> openxr::Result<Option<TrackedKeyboard>> {
        let instance = self.session.instance();
        let keyboard_fp = instance.exts().fb_keyboard_tracking.as_ref().unwrap();
        let now = Instant::now();
        if now >= self.next_query {
            self.next_query = now + Duration::from_secs(1);
            let query = sys::KeyboardTrackingQueryFB {
                ty: sys::KeyboardTrackingQueryFB::TYPE,
                next: ptr::null_mut(),
                flags: sys::KeyboardTrackingQueryFlagsFB::LOCAL,
            };
            // SAFETY: zero is valid for all description fields; the runtime writes the output.
            let mut description: sys::KeyboardTrackingDescriptionFB = unsafe { std::mem::zeroed() };
            unsafe {
                check((keyboard_fp.query_system_tracked_keyboard)(
                    self.session.as_raw(),
                    &query,
                    &mut description,
                ))?;
            }
            if !description
                .flags
                .contains(sys::KeyboardTrackingFlagsFB::EXISTS)
                || !valid_size(description.size)
            {
                self.space = None;
                self.description = None;
                return Ok(None);
            }
            if self.description.as_ref().map(|d| d.tracked_keyboard_id)
                != Some(description.tracked_keyboard_id)
            {
                self.space = None;
                let info = sys::KeyboardSpaceCreateInfoFB {
                    ty: sys::KeyboardSpaceCreateInfoFB::TYPE,
                    next: ptr::null_mut(),
                    tracked_keyboard_id: description.tracked_keyboard_id,
                };
                let mut space = sys::Space::NULL;
                // SAFETY: the output becomes an owned Space retaining the session.
                unsafe {
                    check((keyboard_fp.create_keyboard_space)(
                        self.session.as_raw(),
                        &info,
                        &mut space,
                    ))?;
                    self.space = Some(Space::reference_from_raw(self.session.clone(), space));
                }
            }
            self.description = Some(description);
        }
        let (Some(space), Some(description)) = (&self.space, &self.description) else {
            return Ok(None);
        };
        let location = space.locate(base, time)?;
        if !location.location_flags.contains(
            openxr::SpaceLocationFlags::POSITION_VALID
                | openxr::SpaceLocationFlags::ORIENTATION_VALID,
        ) {
            return Ok(None);
        }
        let keyboard = TrackedKeyboard::new(
            ScreenPose::new(
                Vec3::new(
                    location.pose.position.x,
                    location.pose.position.y,
                    location.pose.position.z,
                ),
                Quat::new(
                    location.pose.orientation.x,
                    location.pose.orientation.y,
                    location.pose.orientation.z,
                    location.pose.orientation.w,
                ),
            ),
            description.size.x,
            description.size.z,
        );
        Ok(Some(keyboard))
    }
}

fn valid_size(size: sys::Vector3f) -> bool {
    size.x.is_finite() && size.z.is_finite() && size.x > 0.0 && size.z > 0.0
}

impl Drop for KeyboardTracking {
    fn drop(&mut self) {
        let extensions = self.session.instance().exts();
        // SAFETY: non-null handles were created by this session and are destroyed only here.
        // Geometry must go before its mesh and layer. Rust then drops the layer before passthrough.
        unsafe {
            if self.geometry != sys::GeometryInstanceFB::NULL {
                (extensions
                    .fb_passthrough
                    .as_ref()
                    .unwrap()
                    .destroy_geometry_instance)(self.geometry);
            }
            (extensions
                .fb_triangle_mesh
                .as_ref()
                .unwrap()
                .destroy_triangle_mesh)(self.mesh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_keyboard_dimensions_must_be_usable() {
        for (x, z, expected) in [
            (0.44, 0.14, true),
            (0.0, 0.14, false),
            (0.44, -1.0, false),
            (f32::NAN, 0.14, false),
            (0.44, f32::INFINITY, false),
        ] {
            assert_eq!(valid_size(sys::Vector3f { x, y: 0.0, z }), expected);
        }
    }
}
