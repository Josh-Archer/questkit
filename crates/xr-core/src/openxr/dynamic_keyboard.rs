//! Quest 3 generic keyboard tracker (Horizon v72+).
//!
//! openxr 0.22 predates these META preview extensions. The small ABI declarations below
//! follow Meta's meta_dynamic_object_{tracker,keyboard}.h (spec version 1), loaded only when
//! both extension names are advertised. Scene/query/space functions use openxr-sys types.
use crate::spatial::{Quat, ScreenPose, TrackedKeyboard, Vec3};
use openxr::{sys, Session, Space, Time, Vulkan};
use std::{
    ffi::c_void,
    ptr,
    time::{Duration, Instant},
};

pub const EXTENSIONS: [&[u8]; 2] = [
    b"XR_META_dynamic_object_tracker",
    b"XR_META_dynamic_object_keyboard",
];
const CREATE: i32 = 1000288001;
const CLASSES: i32 = 1000288002;
const CREATE_RESULT: i32 = 1000288004;
const CLASSES_RESULT: i32 = 1000288005;
const TRACKER_PROPERTIES: i32 = 1000288006;
const OBJECT_COMPONENT: i32 = 1000288007;
const KEYBOARD_CLASS: i32 = 1000587000;
const KEYBOARD_PROPERTIES: i32 = 1000587001;

#[repr(C)]
struct Info {
    ty: sys::StructureType,
    next: *const c_void,
}
#[repr(C)]
struct Classes {
    ty: sys::StructureType,
    next: *const c_void,
    count: u32,
    classes: *const i32,
}
#[repr(C)]
struct Properties {
    ty: sys::StructureType,
    next: *mut c_void,
    supported: sys::Bool32,
}
#[repr(C)]
struct TrackerEvent {
    ty: sys::StructureType,
    next: *const c_void,
    handle: u64,
    result: sys::Result,
}

type Create = unsafe extern "system" fn(sys::Session, *const Info, *mut u64) -> sys::Result;
type Destroy = unsafe extern "system" fn(u64) -> sys::Result;
type SetClasses = unsafe extern "system" fn(u64, *const Classes) -> sys::Result;

fn check(result: sys::Result) -> openxr::Result<()> {
    if result.into_raw() < 0 {
        Err(result)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Empty,
    Creating,
    Configuring,
    Ready,
    Disabled,
}

pub struct DynamicKeyboard {
    session: Session<Vulkan>,
    create: Create,
    destroy: Destroy,
    set_classes: SetClasses,
    handle: u64,
    state: State,
    query: Option<sys::AsyncRequestIdFB>,
    next_query: Instant,
    next_create: Instant,
    space: Option<Space>,
}

impl DynamicKeyboard {
    pub fn new(session: &Session<Vulkan>, system: sys::SystemId) -> openxr::Result<Self> {
        let instance = session.instance();
        let mut keyboard = Properties {
            ty: sys::StructureType::from_raw(KEYBOARD_PROPERTIES),
            next: ptr::null_mut(),
            supported: false.into(),
        };
        let mut tracker = Properties {
            ty: sys::StructureType::from_raw(TRACKER_PROPERTIES),
            next: &mut keyboard as *mut _ as *mut _,
            supported: false.into(),
        };
        let mut properties = sys::SystemProperties {
            ty: sys::SystemProperties::TYPE,
            next: &mut tracker as *mut _ as *mut _,
            ..unsafe { std::mem::zeroed() }
        };
        // SAFETY: correctly typed output chain, writable through this synchronous call.
        unsafe {
            check((instance.fp().get_system_properties)(
                instance.as_raw(),
                system,
                &mut properties,
            ))?;
        }
        if !bool::from(keyboard.supported) || !bool::from(tracker.supported) {
            return Err(sys::Result::ERROR_FEATURE_UNSUPPORTED);
        }
        macro_rules! load {
            ($name:literal, $ty:ty) => {{
                let mut function = None;
                // SAFETY: extension availability was checked before instance creation. The
                // function signature matches Meta's version-1 header and is kept with its session.
                unsafe {
                    check((instance.fp().get_instance_proc_addr)(
                        instance.as_raw(),
                        concat!($name, "\0").as_ptr().cast(),
                        &mut function,
                    ))?;
                    std::mem::transmute::<sys::pfn::VoidFunction, $ty>(
                        function.ok_or(sys::Result::ERROR_FUNCTION_UNSUPPORTED)?,
                    )
                }
            }};
        }
        Ok(Self {
            create: load!("xrCreateDynamicObjectTrackerMETA", Create),
            destroy: load!("xrDestroyDynamicObjectTrackerMETA", Destroy),
            set_classes: load!("xrSetDynamicObjectTrackedClassesMETA", SetClasses),
            session: session.clone(),
            handle: 0,
            state: State::Empty,
            query: None,
            next_query: Instant::now(),
            next_create: Instant::now(),
            space: None,
        })
    }

    /// Called before openxr's generated decoder, which skips unknown META preview events.
    pub fn event(&mut self, buffer: &sys::EventDataBuffer) -> openxr::Result<()> {
        let result = self.event_inner(buffer);
        if result.is_err() {
            self.state = State::Disabled;
            self.space = None;
            self.query = None;
        }
        result
    }

    fn event_inner(&mut self, buffer: &sys::EventDataBuffer) -> openxr::Result<()> {
        match buffer.ty.into_raw() {
            CREATE_RESULT | CLASSES_RESULT => {
                // SAFETY: the runtime's structure type identifies the layout of the event.
                let event = unsafe { &*(buffer as *const _ as *const TrackerEvent) };
                let Some(next) = tracker_event_state(self.state, self.handle, event) else {
                    return Ok(());
                };
                if next == State::Disabled {
                    self.space = None;
                    self.query = None;
                    tracing::warn!(result = ?event.result, "generic keyboard tracking unavailable; check headset tracking setting and spatial permission");
                } else if next == State::Configuring {
                    let class = KEYBOARD_CLASS;
                    let classes = Classes {
                        ty: sys::StructureType::from_raw(CLASSES),
                        next: ptr::null(),
                        count: 1,
                        classes: &class,
                    };
                    // SAFETY: classes is a valid version-1 input structure, live through the call.
                    unsafe {
                        check((self.set_classes)(self.handle, &classes))?;
                    }
                } else if next == State::Ready {
                    tracing::info!("generic keyboard tracking ready");
                }
                self.state = next;
            }
            _ if buffer.ty == sys::StructureType::EVENT_DATA_SPACE_QUERY_RESULTS_AVAILABLE_FB => {
                let event = unsafe {
                    &*(buffer as *const _ as *const sys::EventDataSpaceQueryResultsAvailableFB)
                };
                if self.query == Some(event.request_id) {
                    self.results(event.request_id)?;
                }
            }
            _ if buffer.ty == sys::StructureType::EVENT_DATA_SPACE_QUERY_COMPLETE_FB => {
                let event =
                    unsafe { &*(buffer as *const _ as *const sys::EventDataSpaceQueryCompleteFB) };
                if self.query == Some(event.request_id) {
                    self.query = None;
                    if event.result.into_raw() < 0 {
                        self.space = None;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn results(&mut self, request: sys::AsyncRequestIdFB) -> openxr::Result<()> {
        let fp = self
            .session
            .instance()
            .exts()
            .fb_spatial_entity_query
            .as_ref()
            .unwrap();
        // Query asks for at most one keyboard, but allow extra results defensively.
        let mut spaces: [sys::SpaceQueryResultFB; 16] = unsafe { std::mem::zeroed() };
        let mut results = sys::SpaceQueryResultsFB {
            ty: sys::SpaceQueryResultsFB::TYPE,
            next: ptr::null_mut(),
            result_capacity_input: spaces.len() as u32,
            result_count_output: 0,
            results: spaces.as_mut_ptr(),
        };
        unsafe {
            check((fp.retrieve_space_query_results)(
                self.session.as_raw(),
                request,
                &mut results,
            ))?;
        }
        let mut chosen = None;
        for result in spaces.iter().take(results.result_count_output as usize) {
            if self
                .space
                .as_ref()
                .is_some_and(|space| space.as_raw() == result.space)
            {
                chosen = self.space.take();
            } else {
                // SAFETY: query LOAD returns an owned space associated with our session.
                let space =
                    unsafe { Space::reference_from_raw(self.session.clone(), result.space) };
                if chosen.is_none() {
                    chosen = Some(space);
                }
            }
        }
        self.space = chosen;
        Ok(())
    }

    pub fn update(&mut self, base: &Space, time: Time) -> openxr::Result<Option<TrackedKeyboard>> {
        let now = Instant::now();
        if matches!(self.state, State::Empty | State::Disabled) && now >= self.next_create {
            self.next_create = now + Duration::from_secs(5);
            self.space = None;
            self.query = None;
            if self.handle != 0 {
                unsafe {
                    (self.destroy)(self.handle);
                }
                self.handle = 0;
            }
            let info = Info {
                ty: sys::StructureType::from_raw(CREATE),
                next: ptr::null(),
            };
            unsafe {
                check((self.create)(
                    self.session.as_raw(),
                    &info,
                    &mut self.handle,
                ))?;
            }
            self.state = State::Creating;
        }
        if self.state != State::Ready {
            return Ok(None);
        }
        if self.query.is_none() && now >= self.next_query {
            self.next_query = now + Duration::from_secs(1);
            let filter = sys::SpaceComponentFilterInfoFB {
                ty: sys::SpaceComponentFilterInfoFB::TYPE,
                next: ptr::null(),
                component_type: sys::SpaceComponentTypeFB::from_raw(OBJECT_COMPONENT),
            };
            let info = sys::SpaceQueryInfoFB {
                ty: sys::SpaceQueryInfoFB::TYPE,
                next: ptr::null(),
                query_action: sys::SpaceQueryActionFB::LOAD,
                max_result_count: 1,
                timeout: openxr::Duration::from_nanos(0),
                filter: &filter as *const _ as *const _,
                exclude_filter: ptr::null(),
            };
            let mut request = sys::AsyncRequestIdFB::default();
            let fp = self
                .session
                .instance()
                .exts()
                .fb_spatial_entity_query
                .as_ref()
                .unwrap();
            unsafe {
                check((fp.query_spaces)(
                    self.session.as_raw(),
                    &info as *const _ as *const _,
                    &mut request,
                ))?;
            }
            self.query = Some(request);
        }
        let Some(space) = &self.space else {
            return Ok(None);
        };
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
        unsafe {
            check((fp.get_space_component_status)(
                space.as_raw(),
                sys::SpaceComponentTypeFB::LOCATABLE,
                &mut status,
            ))?;
        }
        if !bool::from(status.enabled) {
            if !bool::from(status.change_pending) {
                let info = sys::SpaceComponentStatusSetInfoFB {
                    ty: sys::SpaceComponentStatusSetInfoFB::TYPE,
                    next: ptr::null(),
                    component_type: sys::SpaceComponentTypeFB::LOCATABLE,
                    enabled: true.into(),
                    timeout: openxr::Duration::from_nanos(0),
                };
                let mut request = sys::AsyncRequestIdFB::default();
                unsafe {
                    check((fp.set_space_component_status)(
                        space.as_raw(),
                        &info,
                        &mut request,
                    ))?;
                }
            }
            return Ok(None);
        }
        let location = space.locate(base, time)?;
        if !location.location_flags.contains(
            openxr::SpaceLocationFlags::POSITION_VALID
                | openxr::SpaceLocationFlags::ORIENTATION_VALID,
        ) {
            return Ok(None);
        }
        let fp = self.session.instance().exts().fb_scene.as_ref().unwrap();
        let mut bounds = sys::Rect3DfFB::default();
        unsafe {
            check((fp.get_space_bounding_box3_d)(
                self.session.as_raw(),
                space.as_raw(),
                &mut bounds,
            ))?;
        }
        Ok(keyboard_from_bounds(location.pose, bounds))
    }
}

/// Ignore events for another tracker, including late results from a destroyed tracker. A
/// configuration result cannot make tracking ready until creation has succeeded.
fn tracker_event_state(current: State, handle: u64, event: &TrackerEvent) -> Option<State> {
    if handle == 0 || handle != event.handle {
        return None;
    }
    let next = match (current, event.ty.into_raw()) {
        (State::Creating, CREATE_RESULT) => State::Configuring,
        (State::Configuring, CLASSES_RESULT) => State::Ready,
        _ => return None,
    };
    Some(if event.result.into_raw() < 0 {
        State::Disabled
    } else {
        next
    })
}

fn keyboard_from_bounds(pose: sys::Posef, bounds: sys::Rect3DfFB) -> Option<TrackedKeyboard> {
    let width = bounds.extent.width;
    let depth = bounds.extent.height;
    let center = Vec3::new(
        bounds.offset.x + width * 0.5,
        bounds.offset.y + depth * 0.5,
        bounds.offset.z + bounds.extent.depth,
    );
    if ![width, depth, center.x, center.y, center.z]
        .iter()
        .all(|v| v.is_finite())
        || width <= 0.0
        || depth <= 0.0
    {
        return None;
    }
    // Scene keyboard spaces lie in XY with +Z above the keys. The rest of the client
    // uses XZ with +Y up, so apply the rotation once at the tracking boundary.
    let orientation = Quat::new(
        pose.orientation.x,
        pose.orientation.y,
        pose.orientation.z,
        pose.orientation.w,
    );
    Some(TrackedKeyboard::new(
        ScreenPose::new(
            Vec3::new(pose.position.x, pose.position.y, pose.position.z)
                + orientation.rotate_vec(center),
            orientation * Quat::from_axis_angle(Vec3::UNIT_X, std::f32::consts::FRAC_PI_2),
        ),
        width,
        depth,
    ))
}

impl Drop for DynamicKeyboard {
    fn drop(&mut self) {
        // All loaded spaces must be gone before their tracker. Session is retained by self.
        self.space = None;
        if self.handle != 0 {
            unsafe {
                (self.destroy)(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asynchronous_tracker_results_require_the_expected_handle_and_order() {
        let mut event = TrackerEvent {
            ty: sys::StructureType::from_raw(CLASSES_RESULT),
            next: ptr::null(),
            handle: 7,
            result: sys::Result::SUCCESS,
        };
        assert_eq!(tracker_event_state(State::Creating, 7, &event), None);
        event.ty = sys::StructureType::from_raw(CREATE_RESULT);
        assert_eq!(tracker_event_state(State::Creating, 0, &event), None);
        assert_eq!(tracker_event_state(State::Creating, 8, &event), None);
        assert_eq!(
            tracker_event_state(State::Creating, 7, &event),
            Some(State::Configuring)
        );
        event.ty = sys::StructureType::from_raw(CLASSES_RESULT);
        assert_eq!(
            tracker_event_state(State::Configuring, 7, &event),
            Some(State::Ready)
        );
        event.result = sys::Result::ERROR_INITIALIZATION_FAILED;
        assert_eq!(
            tracker_event_state(State::Configuring, 7, &event),
            Some(State::Disabled)
        );
        assert_eq!(tracker_event_state(State::Ready, 7, &event), None);
    }

    #[test]
    fn scene_bounds_convert_to_key_surface_in_client_axes() {
        let bounds = sys::Rect3DfFB {
            offset: sys::Offset3DfFB {
                x: -0.22,
                y: -0.07,
                z: -0.02,
            },
            extent: sys::Extent3DfFB {
                width: 0.44,
                height: 0.14,
                depth: 0.02,
            },
        };
        let keyboard = keyboard_from_bounds(sys::Posef::IDENTITY, bounds).unwrap();
        assert_eq!(keyboard.pose.position, Vec3::ZERO);
        assert!(
            (keyboard.pose.orientation.rotate_vec(Vec3::UNIT_Y) - Vec3::UNIT_Z).length() < 1e-5
        );
        assert!(keyboard.is_over(Vec3::new(0.1, 0.02, 0.03), 0.0, 0.06));
        assert!(keyboard_from_bounds(sys::Posef::IDENTITY, sys::Rect3DfFB::default()).is_none());
    }
}
