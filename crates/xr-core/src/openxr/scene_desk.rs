//! Quest Space Setup TABLE planes (XR_FB_scene + XR_FB_spatial_entity(_query)).
//!
//! Loads every scene space with semantic labels once, keeps the ones labelled TABLE (or
//! DESK, which older runtimes report instead), and each frame reports their horizontal
//! extents as [`TablePlane`]s in the caller's base space. The desk is chosen from these in
//! pure code (`spatial::desk_source`). Requires `com.oculus.permission.USE_SCENE`; without it,
//! or without Space Setup, the query returns nothing and palms remain the only desk source.
use crate::spatial::{Quat, ScreenPose, TablePlane, Vec3};
use openxr::{sys, Session, Space, Time, Vulkan};
use std::{
    ptr,
    time::{Duration, Instant},
};

/// How often to re-query when no TABLE plane has been found yet.
const REQUERY_INTERVAL: Duration = Duration::from_secs(10);
/// At most this many scene spaces are loaded per query.
const MAX_SCENE_SPACES: usize = 64;

fn check(result: sys::Result) -> openxr::Result<()> {
    if result.into_raw() < 0 {
        Err(result)
    } else {
        Ok(())
    }
}

/// Whether a comma-separated XR_FB_scene label list names a table.
pub(crate) fn labels_name_a_table(labels: &str) -> bool {
    labels
        .split(',')
        .any(|l| matches!(l.trim(), "TABLE" | "DESK"))
}

pub struct SceneDesk {
    session: Session<Vulkan>,
    query: Option<sys::AsyncRequestIdFB>,
    next_query: Instant,
    tables: Vec<Space>,
    logged: bool,
}

impl SceneDesk {
    pub fn new(session: &Session<Vulkan>) -> Self {
        Self {
            session: session.clone(),
            query: None,
            next_query: Instant::now(),
            tables: Vec::new(),
            logged: false,
        }
    }

    /// Feed every polled event. Errors only disable this frame's work.
    pub fn event(&mut self, buffer: &sys::EventDataBuffer) {
        if let Err(error) = self.event_inner(buffer) {
            tracing::warn!(?error, "scene TABLE query failed; desk falls back to palms");
            self.query = None;
        }
    }

    fn event_inner(&mut self, buffer: &sys::EventDataBuffer) -> openxr::Result<()> {
        if buffer.ty == sys::StructureType::EVENT_DATA_SPACE_QUERY_RESULTS_AVAILABLE_FB {
            // SAFETY: the structure type identifies the event layout.
            let event = unsafe {
                &*(buffer as *const _ as *const sys::EventDataSpaceQueryResultsAvailableFB)
            };
            if self.query == Some(event.request_id) {
                self.results(event.request_id)?;
            }
        } else if buffer.ty == sys::StructureType::EVENT_DATA_SPACE_QUERY_COMPLETE_FB {
            // SAFETY: as above.
            let event =
                unsafe { &*(buffer as *const _ as *const sys::EventDataSpaceQueryCompleteFB) };
            if self.query == Some(event.request_id) {
                self.query = None;
            }
        }
        Ok(())
    }

    fn labels(&self, space: sys::Space) -> openxr::Result<String> {
        let fp = self.session.instance().exts().fb_scene.as_ref().unwrap();
        let mut labels = sys::SemanticLabelsFB {
            ty: sys::SemanticLabelsFB::TYPE,
            next: ptr::null(),
            buffer_capacity_input: 0,
            buffer_count_output: 0,
            buffer: ptr::null_mut(),
        };
        // SAFETY: two-call idiom; the first call writes only the count.
        unsafe {
            check((fp.get_space_semantic_labels)(
                self.session.as_raw(),
                space,
                &mut labels,
            ))?;
        }
        let mut buffer = vec![0u8; labels.buffer_count_output as usize];
        if buffer.is_empty() {
            return Ok(String::new());
        }
        labels.buffer_capacity_input = buffer.len() as u32;
        labels.buffer = buffer.as_mut_ptr().cast();
        // SAFETY: buffer is writable for buffer_capacity_input bytes through this call.
        unsafe {
            check((fp.get_space_semantic_labels)(
                self.session.as_raw(),
                space,
                &mut labels,
            ))?;
        }
        buffer.truncate(labels.buffer_count_output as usize);
        while buffer.last() == Some(&0) {
            buffer.pop();
        }
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    }

    fn results(&mut self, request: sys::AsyncRequestIdFB) -> openxr::Result<()> {
        let fp = self
            .session
            .instance()
            .exts()
            .fb_spatial_entity_query
            .as_ref()
            .unwrap();
        // SAFETY: plain-old-data output array.
        let mut spaces: [sys::SpaceQueryResultFB; MAX_SCENE_SPACES] = unsafe { std::mem::zeroed() };
        let mut results = sys::SpaceQueryResultsFB {
            ty: sys::SpaceQueryResultsFB::TYPE,
            next: ptr::null_mut(),
            result_capacity_input: spaces.len() as u32,
            result_count_output: 0,
            results: spaces.as_mut_ptr(),
        };
        // SAFETY: results points at a writable array of result_capacity_input entries.
        unsafe {
            check((fp.retrieve_space_query_results)(
                self.session.as_raw(),
                request,
                &mut results,
            ))?;
        }
        for result in spaces.iter().take(results.result_count_output as usize) {
            if self.tables.iter().any(|s| s.as_raw() == result.space) {
                continue;
            }
            let is_table = self
                .labels(result.space)
                .map(|l| labels_name_a_table(&l))
                .unwrap_or(false);
            if is_table {
                // SAFETY: query LOAD returns a space owned by our session.
                self.tables
                    .push(unsafe { Space::reference_from_raw(self.session.clone(), result.space) });
            }
        }
        if !self.tables.is_empty() && !self.logged {
            self.logged = true;
            tracing::info!(count = self.tables.len(), "Space Setup TABLE planes loaded");
        }
        Ok(())
    }

    fn start_query(&mut self) -> openxr::Result<()> {
        let filter = sys::SpaceComponentFilterInfoFB {
            ty: sys::SpaceComponentFilterInfoFB::TYPE,
            next: ptr::null(),
            component_type: sys::SpaceComponentTypeFB::SEMANTIC_LABELS,
        };
        let info = sys::SpaceQueryInfoFB {
            ty: sys::SpaceQueryInfoFB::TYPE,
            next: ptr::null(),
            query_action: sys::SpaceQueryActionFB::LOAD,
            max_result_count: MAX_SCENE_SPACES as u32,
            timeout: openxr::Duration::from_nanos(0),
            filter: &filter as *const _ as *const _,
            exclude_filter: ptr::null(),
        };
        let fp = self
            .session
            .instance()
            .exts()
            .fb_spatial_entity_query
            .as_ref()
            .unwrap();
        let mut request = sys::AsyncRequestIdFB::default();
        // SAFETY: info and filter live through this synchronous call.
        unsafe {
            check((fp.query_spaces)(
                self.session.as_raw(),
                &info as *const _ as *const _,
                &mut request,
            ))?;
        }
        self.query = Some(request);
        Ok(())
    }

    /// Ensures a space's LOCATABLE component is on; returns whether it is usable now.
    fn locatable(&self, space: &Space) -> openxr::Result<bool> {
        let fp = self
            .session
            .instance()
            .exts()
            .fb_spatial_entity
            .as_ref()
            .unwrap();
        let mut status = sys::SpaceComponentStatusFB {
            ty: sys::SpaceComponentStatusFB::TYPE,
            next: ptr::null_mut(),
            enabled: false.into(),
            change_pending: false.into(),
        };
        // SAFETY: status is a valid output structure.
        unsafe {
            check((fp.get_space_component_status)(
                space.as_raw(),
                sys::SpaceComponentTypeFB::LOCATABLE,
                &mut status,
            ))?;
        }
        if bool::from(status.enabled) {
            return Ok(true);
        }
        if !bool::from(status.change_pending) {
            let info = sys::SpaceComponentStatusSetInfoFB {
                ty: sys::SpaceComponentStatusSetInfoFB::TYPE,
                next: ptr::null(),
                component_type: sys::SpaceComponentTypeFB::LOCATABLE,
                enabled: true.into(),
                timeout: openxr::Duration::from_nanos(0),
            };
            let mut request = sys::AsyncRequestIdFB::default();
            // SAFETY: info lives through the call.
            unsafe {
                check((fp.set_space_component_status)(
                    space.as_raw(),
                    &info,
                    &mut request,
                ))?;
            }
        }
        Ok(false)
    }

    /// TABLE planes located in `base` at `time`. Empty until Space Setup data loads.
    pub fn update(&mut self, base: &Space, time: Time) -> Vec<TablePlane> {
        let now = Instant::now();
        if self.tables.is_empty() && self.query.is_none() && now >= self.next_query {
            self.next_query = now + REQUERY_INTERVAL;
            if let Err(error) = self.start_query() {
                tracing::debug!(?error, "scene space query not started");
            }
        }
        let fp = self.session.instance().exts().fb_scene.as_ref().unwrap();
        let mut planes = Vec::new();
        for space in &self.tables {
            if !self.locatable(space).unwrap_or(false) {
                continue;
            }
            let Ok(location) = space.locate(base, time) else {
                continue;
            };
            if !location.location_flags.contains(
                openxr::SpaceLocationFlags::POSITION_VALID
                    | openxr::SpaceLocationFlags::ORIENTATION_VALID,
            ) {
                continue;
            }
            let mut bounds = sys::Rect2Df::default();
            // SAFETY: bounds is a valid output structure.
            let ok = unsafe {
                (fp.get_space_bounding_box2_d)(self.session.as_raw(), space.as_raw(), &mut bounds)
            };
            if ok.into_raw() < 0 {
                continue;
            }
            planes.extend(table_plane_from_bounds(location.pose, bounds));
        }
        planes
    }
}

/// Converts a scene plane (local XY plane, +Z normal) and its 2D bounds into a
/// [`TablePlane`] in client axes (+Y normal, X width, Z depth).
pub(crate) fn table_plane_from_bounds(
    pose: sys::Posef,
    bounds: sys::Rect2Df,
) -> Option<TablePlane> {
    let (w, h) = (bounds.extent.width, bounds.extent.height);
    let cx = bounds.offset.x + w * 0.5;
    let cy = bounds.offset.y + h * 0.5;
    if ![w, h, cx, cy].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let orientation = Quat::new(
        pose.orientation.x,
        pose.orientation.y,
        pose.orientation.z,
        pose.orientation.w,
    );
    let position = Vec3::new(pose.position.x, pose.position.y, pose.position.z)
        + orientation.rotate_vec(Vec3::new(cx, cy, 0.0));
    Some(TablePlane {
        pose: ScreenPose::new(
            position,
            orientation * Quat::from_axis_angle(Vec3::UNIT_X, std::f32::consts::FRAC_PI_2),
        ),
        width_m: w,
        depth_m: h,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_and_desk_labels_are_tables() {
        assert!(labels_name_a_table("TABLE"));
        assert!(labels_name_a_table("OTHER,DESK"));
        assert!(!labels_name_a_table("FLOOR,WALL_FACE"));
        assert!(!labels_name_a_table(""));
    }

    #[test]
    fn scene_plane_bounds_become_an_upward_table_plane() {
        // A Quest table plane: scene +Z points up (rotate -90 deg about X from identity).
        let q = Quat::from_axis_angle(Vec3::UNIT_X, -std::f32::consts::FRAC_PI_2);
        let pose = sys::Posef {
            orientation: sys::Quaternionf {
                x: q.x,
                y: q.y,
                z: q.z,
                w: q.w,
            },
            position: sys::Vector3f {
                x: 0.0,
                y: 0.74,
                z: -0.6,
            },
        };
        let bounds = sys::Rect2Df {
            offset: sys::Offset2Df { x: -0.6, y: -0.3 },
            extent: sys::Extent2Df {
                width: 1.2,
                height: 0.6,
            },
        };
        let plane = table_plane_from_bounds(pose, bounds).unwrap();
        let up = plane.pose.orientation.rotate_vec(Vec3::UNIT_Y);
        assert!((up - Vec3::UNIT_Y).length() < 1e-5, "{up:?}");
        assert!((plane.pose.position - Vec3::new(0.0, 0.74, -0.6)).length() < 1e-5);
        assert_eq!((plane.width_m, plane.depth_m), (1.2, 0.6));
        assert!(table_plane_from_bounds(pose, sys::Rect2Df::default()).is_none());
    }
}
