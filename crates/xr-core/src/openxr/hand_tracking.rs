//! OpenXR XR_EXT_hand_tracking support, skeletal joint querying, and mock hand backend.

use crate::error::{XrError, XrResult};
use crate::spatial::interaction::{
    HandAimState, HandJoint, HandJointData, HandTrackingData, Handedness, HAND_JOINT_COUNT,
};
use crate::spatial::pose::{Quat, Vec3};

/// Backend interface for querying skeletal hand tracking data from OpenXR `XR_EXT_hand_tracking`.
pub trait HandTrackingBackend: Send + Sync {
    /// Returns true if `XR_EXT_hand_tracking` extension is supported by the OpenXR runtime.
    fn is_supported(&self) -> bool;

    /// Returns true if hand tracking is actively tracking and delivering data.
    fn is_active(&self) -> bool;

    /// Locates all 26 skeletal joints in reference space at the given display time.
    fn locate_joints(
        &mut self,
        hand: Handedness,
        time: openxr::Time,
        stage_space: Option<&openxr::Space>,
    ) -> XrResult<Option<HandTrackingData>>;
}

/// Simulated hand tracking backend for testing gestures, hysteresis, and manipulation.
#[derive(Debug, Clone)]
pub struct MockHandTrackingBackend {
    supported: bool,
    left_hand: Option<HandTrackingData>,
    right_hand: Option<HandTrackingData>,
    locate_count: usize,
}

impl Default for MockHandTrackingBackend {
    fn default() -> Self {
        Self::new(true)
    }
}

impl MockHandTrackingBackend {
    /// Creates a mock hand tracking backend with specified support flag.
    pub fn new(supported: bool) -> Self {
        Self {
            supported,
            left_hand: None,
            right_hand: None,
            locate_count: 0,
        }
    }

    /// Creates an unsupported hand tracking backend (e.g. running on 3DoF headset or desktop).
    pub fn unsupported() -> Self {
        Self::new(false)
    }

    /// Sets the full joint skeletal data for a hand.
    pub fn set_hand_data(&mut self, hand: Handedness, data: Option<HandTrackingData>) {
        match hand {
            Handedness::Left => self.left_hand = data,
            Handedness::Right => self.right_hand = data,
        }
    }

    /// Helper to simulate an open hand with fingertips spread out.
    pub fn simulate_open_hand(&mut self, hand: Handedness, palm_pos: Vec3, dir: Vec3) {
        let mut data = HandTrackingData {
            is_active: true,
            ..Default::default()
        };

        let forward = dir.normalize();
        let wrist_pos = palm_pos - forward * 0.08;
        data.joint_mut(HandJoint::Wrist).position = wrist_pos;
        data.joint_mut(HandJoint::Wrist).is_valid = true;

        data.joint_mut(HandJoint::Palm).position = palm_pos;
        data.joint_mut(HandJoint::Palm).is_valid = true;

        data.joint_mut(HandJoint::IndexProximal).position = palm_pos + forward * 0.05;
        data.joint_mut(HandJoint::IndexProximal).is_valid = true;

        // Thumb and index tips far apart (80mm)
        data.joint_mut(HandJoint::ThumbTip).position = palm_pos + Vec3::new(-0.04, 0.0, 0.04);
        data.joint_mut(HandJoint::ThumbTip).is_valid = true;

        data.joint_mut(HandJoint::IndexTip).position =
            palm_pos + forward * 0.09 + Vec3::new(0.02, 0.0, 0.0);
        data.joint_mut(HandJoint::IndexTip).is_valid = true;

        data.joint_mut(HandJoint::MiddleTip).position = palm_pos + forward * 0.10;
        data.joint_mut(HandJoint::MiddleTip).is_valid = true;

        data.joint_mut(HandJoint::RingTip).position = palm_pos + forward * 0.09;
        data.joint_mut(HandJoint::RingTip).is_valid = true;

        data.joint_mut(HandJoint::LittleTip).position = palm_pos + forward * 0.08;
        data.joint_mut(HandJoint::LittleTip).is_valid = true;

        self.set_hand_data(hand, Some(data));
    }

    /// Helper to simulate a pinch gesture with thumb and index tips at a specific distance.
    pub fn simulate_pinch(&mut self, hand: Handedness, pinch_pos: Vec3, dir: Vec3, distance: f32) {
        let mut data = HandTrackingData {
            is_active: true,
            ..Default::default()
        };

        let forward = dir.normalize();
        let palm_pos = pinch_pos - forward * 0.06;
        let wrist_pos = palm_pos - forward * 0.08;

        data.joint_mut(HandJoint::Wrist).position = wrist_pos;
        data.joint_mut(HandJoint::Wrist).is_valid = true;

        data.joint_mut(HandJoint::Palm).position = palm_pos;
        data.joint_mut(HandJoint::Palm).is_valid = true;

        data.joint_mut(HandJoint::IndexProximal).position = palm_pos + forward * 0.04;
        data.joint_mut(HandJoint::IndexProximal).is_valid = true;

        let half_d = distance * 0.5;
        data.joint_mut(HandJoint::ThumbTip).position = pinch_pos + Vec3::new(-half_d, 0.0, 0.0);
        data.joint_mut(HandJoint::ThumbTip).is_valid = true;

        data.joint_mut(HandJoint::IndexTip).position = pinch_pos + Vec3::new(half_d, 0.0, 0.0);
        data.joint_mut(HandJoint::IndexTip).is_valid = true;

        // Middle/ring/little fingertips loosely open
        data.joint_mut(HandJoint::MiddleTip).position = palm_pos + forward * 0.08;
        data.joint_mut(HandJoint::MiddleTip).is_valid = true;
        data.joint_mut(HandJoint::RingTip).position = palm_pos + forward * 0.07;
        data.joint_mut(HandJoint::RingTip).is_valid = true;
        data.joint_mut(HandJoint::LittleTip).position = palm_pos + forward * 0.06;
        data.joint_mut(HandJoint::LittleTip).is_valid = true;

        self.set_hand_data(hand, Some(data));
    }

    /// Helper to simulate a fist grab with all fingertips curled into the palm.
    pub fn simulate_fist_grab(&mut self, hand: Handedness, palm_pos: Vec3, curl_dist: f32) {
        let mut data = HandTrackingData {
            is_active: true,
            ..Default::default()
        };

        let wrist_pos = palm_pos - Vec3::new(0.0, 0.0, -0.08);
        data.joint_mut(HandJoint::Wrist).position = wrist_pos;
        data.joint_mut(HandJoint::Wrist).is_valid = true;

        data.joint_mut(HandJoint::Palm).position = palm_pos;
        data.joint_mut(HandJoint::Palm).is_valid = true;

        data.joint_mut(HandJoint::IndexProximal).position = palm_pos + Vec3::new(0.0, 0.0, -0.03);
        data.joint_mut(HandJoint::IndexProximal).is_valid = true;

        // All tips curled close to palm at curl_dist
        data.joint_mut(HandJoint::ThumbTip).position = palm_pos + Vec3::new(-curl_dist, 0.0, 0.0);
        data.joint_mut(HandJoint::ThumbTip).is_valid = true;

        data.joint_mut(HandJoint::IndexTip).position = palm_pos + Vec3::new(0.0, curl_dist, 0.0);
        data.joint_mut(HandJoint::IndexTip).is_valid = true;

        data.joint_mut(HandJoint::MiddleTip).position = palm_pos + Vec3::new(0.0, 0.0, -curl_dist);
        data.joint_mut(HandJoint::MiddleTip).is_valid = true;

        data.joint_mut(HandJoint::RingTip).position = palm_pos + Vec3::new(curl_dist, 0.0, 0.0);
        data.joint_mut(HandJoint::RingTip).is_valid = true;

        data.joint_mut(HandJoint::LittleTip).position = palm_pos + Vec3::new(0.0, -curl_dist, 0.0);
        data.joint_mut(HandJoint::LittleTip).is_valid = true;

        self.set_hand_data(hand, Some(data));
    }

    /// Returns the number of times `locate_joints` has been called.
    pub fn locate_count(&self) -> usize {
        self.locate_count
    }
}

impl HandTrackingBackend for MockHandTrackingBackend {
    fn is_supported(&self) -> bool {
        self.supported
    }

    fn is_active(&self) -> bool {
        self.supported && (self.left_hand.is_some() || self.right_hand.is_some())
    }

    fn locate_joints(
        &mut self,
        hand: Handedness,
        _time: openxr::Time,
        _stage_space: Option<&openxr::Space>,
    ) -> XrResult<Option<HandTrackingData>> {
        self.locate_count += 1;
        if !self.supported {
            return Ok(None);
        }

        match hand {
            Handedness::Left => Ok(self.left_hand.clone()),
            Handedness::Right => Ok(self.right_hand.clone()),
        }
    }
}

/// Live OpenXR `XR_EXT_hand_tracking` backend managing native hand trackers and joint queries.
pub struct OpenXrHandTrackingBackend {
    left_tracker: Option<openxr::HandTracker>,
    right_tracker: Option<openxr::HandTracker>,
    supported: bool,
    locate: openxr::sys::pfn::LocateHandJointsEXT,
    aim_enabled: bool,
}

impl OpenXrHandTrackingBackend {
    /// Initializes OpenXR hand trackers for left and right hands.
    pub fn try_init<G: openxr::Graphics>(session: &openxr::Session<G>) -> XrResult<Self> {
        let left_tracker = session
            .create_hand_tracker(openxr::Hand::LEFT)
            .map_err(|e| XrError::OpenXr(format!("Failed to create left hand tracker: {e:?}")))?;

        let right_tracker = session
            .create_hand_tracker(openxr::Hand::RIGHT)
            .map_err(|e| XrError::OpenXr(format!("Failed to create right hand tracker: {e:?}")))?;

        tracing::info!(
            native_pinch_aim = session.instance().exts().fb_hand_tracking_aim.is_some(),
            "OpenXR hand trackers initialized"
        );

        Ok(Self {
            left_tracker: Some(left_tracker),
            right_tracker: Some(right_tracker),
            supported: true,
            locate: session
                .instance()
                .exts()
                .ext_hand_tracking
                .as_ref()
                .unwrap()
                .locate_hand_joints,
            aim_enabled: session.instance().exts().fb_hand_tracking_aim.is_some(),
        })
    }
}

impl HandTrackingBackend for OpenXrHandTrackingBackend {
    fn is_supported(&self) -> bool {
        self.supported
    }

    fn is_active(&self) -> bool {
        self.supported && (self.left_tracker.is_some() || self.right_tracker.is_some())
    }

    fn locate_joints(
        &mut self,
        hand: Handedness,
        time: openxr::Time,
        stage_space: Option<&openxr::Space>,
    ) -> XrResult<Option<HandTrackingData>> {
        if !self.supported {
            return Ok(None);
        }

        let stage = match stage_space {
            Some(s) => s,
            None => return Ok(None),
        };

        let tracker = match hand {
            Handedness::Left => match &self.left_tracker {
                Some(t) => t,
                None => return Ok(None),
            },
            Handedness::Right => match &self.right_tracker {
                Some(t) => t,
                None => return Ok(None),
            },
        };

        // The safe openxr-rs joint-only helper supplies a null output chain and silently
        // omits XR_FB_hand_tracking_aim. Query joints and native pinches together instead.
        // SAFETY: tracker and stage belong to the live session owned by OpenXrContext.
        unsafe {
            locate_sample(
                self.locate,
                tracker.as_raw(),
                stage.as_raw(),
                time,
                self.aim_enabled,
            )
        }
    }
}

/// Query one sample. Keeping the FFI boundary here lets tests emulate the runtime's
/// output chain, rather than bypassing the production conversion with a joint mock.
unsafe fn locate_sample(
    locate: openxr::sys::pfn::LocateHandJointsEXT,
    tracker: openxr::sys::HandTrackerEXT,
    space: openxr::sys::Space,
    time: openxr::Time,
    aim_enabled: bool,
) -> XrResult<Option<HandTrackingData>> {
    use openxr::sys;
    let mut joints = [sys::HandJointLocationEXT::default(); HAND_JOINT_COUNT];
    let mut aim = sys::HandTrackingAimStateFB {
        ty: sys::HandTrackingAimStateFB::TYPE,
        next: std::ptr::null_mut(),
        status: sys::HandTrackingAimFlagsFB::EMPTY,
        aim_pose: sys::Posef::IDENTITY,
        pinch_strength_index: 0.0,
        pinch_strength_middle: 0.0,
        pinch_strength_ring: 0.0,
        pinch_strength_little: 0.0,
    };
    let info = sys::HandJointsLocateInfoEXT {
        ty: sys::HandJointsLocateInfoEXT::TYPE,
        next: std::ptr::null(),
        base_space: space,
        time,
    };
    let mut locations = sys::HandJointLocationsEXT {
        ty: sys::HandJointLocationsEXT::TYPE,
        next: if aim_enabled {
            (&mut aim as *mut sys::HandTrackingAimStateFB).cast()
        } else {
            std::ptr::null_mut()
        },
        is_active: false.into(),
        joint_count: HAND_JOINT_COUNT as u32,
        joint_locations: joints.as_mut_ptr(),
    };
    // SAFETY: all output storage and the optional next chain live through this call.
    let result = unsafe { locate(tracker, &info, &mut locations) };
    if result.into_raw() < 0 {
        return Err(XrError::OpenXr(format!(
            "Failed to locate hand joints: {result:?}"
        )));
    }
    if !bool::from(locations.is_active) {
        tracing::debug!(target: "linux_quest_client::scroll", "Hand tracking inactive");
        return Ok(None);
    }
    let aim = aim_enabled.then(|| {
        use sys::HandTrackingAimFlagsFB as Flags;
        HandAimState {
            valid: aim.status.contains(Flags::VALID),
            system_gesture: aim.status.contains(Flags::SYSTEM_GESTURE),
            index_pinching: aim.status.contains(Flags::INDEX_PINCHING),
            middle_pinching: aim.status.contains(Flags::MIDDLE_PINCHING),
            middle_strength: aim.pinch_strength_middle,
            pose: crate::spatial::ScreenPose::new(
                Vec3::from(aim.aim_pose.position),
                Quat::from(aim.aim_pose.orientation),
            ),
        }
    });
    Ok(Some(HandTrackingData {
        is_active: true,
        aim,
        joints: joints.map(|loc| HandJointData {
            position: Vec3::from(loc.pose.position),
            orientation: if loc
                .location_flags
                .contains(sys::SpaceLocationFlags::ORIENTATION_VALID)
            {
                Quat::from(loc.pose.orientation)
            } else {
                Quat::IDENTITY
            },
            radius: loc.radius,
            is_valid: loc
                .location_flags
                .contains(sys::SpaceLocationFlags::POSITION_VALID),
        }),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn runtime_sample(
        _: openxr::sys::HandTrackerEXT,
        info: *const openxr::sys::HandJointsLocateInfoEXT,
        output: *mut openxr::sys::HandJointLocationsEXT,
    ) -> openxr::sys::Result {
        use openxr::sys;
        // SAFETY: locate_sample supplies the initialized header and 26 output joints.
        let (info, output) = unsafe { (&*info, &mut *output) };
        assert_eq!(output.ty, sys::HandJointLocationsEXT::TYPE);
        assert_eq!(output.joint_count, HAND_JOINT_COUNT as u32);
        if info.time.as_nanos() < 0 {
            return sys::Result::ERROR_TIME_INVALID;
        }
        output.is_active = (info.time.as_nanos() != 0).into();
        for i in 0..HAND_JOINT_COUNT {
            unsafe {
                *output.joint_locations.add(i) = sys::HandJointLocationEXT {
                    location_flags: sys::SpaceLocationFlags::POSITION_VALID,
                    pose: sys::Posef::IDENTITY,
                    radius: 0.008,
                };
            }
        }
        if !output.next.is_null() {
            let aim = unsafe { &mut *output.next.cast::<sys::HandTrackingAimStateFB>() };
            assert_eq!(aim.ty, sys::HandTrackingAimStateFB::TYPE);
            assert!(aim.next.is_null());
            aim.status =
                sys::HandTrackingAimFlagsFB::VALID | sys::HandTrackingAimFlagsFB::MIDDLE_PINCHING;
            aim.pinch_strength_middle = 0.95;
            aim.aim_pose.position.y = 1.3;
        }
        sys::Result::SUCCESS
    }

    #[test]
    fn native_query_chains_aim_and_preserves_position_only_joints() {
        use openxr::sys::{HandTrackerEXT, Handle, Space};
        for enabled in [true, false] {
            let sample = unsafe {
                locate_sample(
                    runtime_sample,
                    HandTrackerEXT::NULL,
                    Space::NULL,
                    openxr::Time::from_nanos(1),
                    enabled,
                )
            }
            .unwrap()
            .unwrap();
            assert!(sample.joint(HandJoint::MiddleTip).is_valid);
            assert_eq!(
                sample.joint(HandJoint::MiddleTip).orientation,
                Quat::IDENTITY
            );
            assert_eq!(sample.aim.is_some(), enabled);
            if let Some(aim) = sample.aim {
                assert!(aim.valid && aim.middle_pinching && !aim.index_pinching);
                assert_eq!(aim.middle_strength, 0.95);
                assert_eq!(aim.pose.position.y, 1.3);
            }
        }
        assert!(unsafe {
            locate_sample(
                runtime_sample,
                HandTrackerEXT::NULL,
                Space::NULL,
                openxr::Time::from_nanos(0),
                true,
            )
        }
        .unwrap()
        .is_none());
        assert!(unsafe {
            locate_sample(
                runtime_sample,
                HandTrackerEXT::NULL,
                Space::NULL,
                openxr::Time::from_nanos(-1),
                true,
            )
        }
        .is_err());
    }

    #[test]
    fn test_mock_hand_tracking_lifecycle_and_helpers() {
        let mut backend = MockHandTrackingBackend::new(true);
        assert!(backend.is_supported());
        assert!(!backend.is_active());

        // Simulate open hand
        backend.simulate_open_hand(
            Handedness::Right,
            Vec3::new(0.0, 1.2, -0.3),
            Vec3::new(0.0, 0.0, -1.0),
        );
        assert!(backend.is_active());

        let right_data = backend
            .locate_joints(Handedness::Right, openxr::Time::from_nanos(0), None)
            .unwrap()
            .expect("Right hand data should be present");

        assert!(right_data.is_active);
        assert_eq!(backend.locate_count(), 1);
        assert!(right_data.joint(HandJoint::ThumbTip).is_valid);
        assert!(right_data.joint(HandJoint::IndexTip).is_valid);

        // Unsupported backend returns None
        let mut unsupported = MockHandTrackingBackend::unsupported();
        assert!(!unsupported.is_supported());
        assert!(!unsupported.is_active());
        assert!(unsupported
            .locate_joints(Handedness::Right, openxr::Time::from_nanos(0), None)
            .unwrap()
            .is_none());
    }
}
