//! Android zero-copy presentation: MediaCodec produces buffers into the native window;
//! Android manages the AHardwareBuffer-backed BufferQueue consumed by the XR compositor.
//! Do not acquire/wait/release images on this surface swapchain from the application.
use crate::error::{MediaError, MediaResult};
use openxr::sys::Handle as _;
use openxr::{Session, Swapchain, Vulkan};
/// Creates an Android surface swapchain (the decoder renders straight into its surface) of
/// `width` x `height` and returns it with its native window.
#[cfg(target_os = "android")]
pub fn create_surface_swapchain(
    session: &Session<Vulkan>,
    pfn: openxr::raw::AndroidSurfaceSwapchainKHR,
    supports_fb_surface_create: bool,
    width: u32,
    height: u32,
) -> MediaResult<(Swapchain<Vulkan>, ndk::native_window::NativeWindow)> {
    let fb_surface_create_info = openxr::sys::AndroidSurfaceSwapchainCreateInfoFB {
        ty: openxr::sys::StructureType::ANDROID_SURFACE_SWAPCHAIN_CREATE_INFO_FB,
        next: std::ptr::null(),
        create_flags: openxr::sys::AndroidSurfaceSwapchainFlagsFB::USE_TIMESTAMPS,
    };
    let info = openxr::sys::SwapchainCreateInfo {
        ty: openxr::sys::StructureType::SWAPCHAIN_CREATE_INFO,
        next: if supports_fb_surface_create {
            &fb_surface_create_info as *const _ as *const std::ffi::c_void
        } else {
            std::ptr::null()
        },
        create_flags: openxr::sys::SwapchainCreateFlags::EMPTY,
        usage_flags: openxr::sys::SwapchainUsageFlags::COLOR_ATTACHMENT
            | openxr::sys::SwapchainUsageFlags::SAMPLED,
        // The Android Surface (MediaCodec output) owns the image format and layout;
        // Horizon OS rejects Vulkan values here ("info format validation failure").
        format: 0,
        sample_count: 0,
        width,
        height,
        face_count: 0,
        array_size: 0,
        mip_count: 0,
    };
    let mut raw_swapchain = openxr::sys::Swapchain::NULL;
    let mut surface: openxr::sys::platform::jobject = std::ptr::null_mut();
    let res = unsafe {
        (pfn.create_swapchain_android_surface)(
            session.as_raw(),
            &info,
            &mut raw_swapchain,
            &mut surface,
        )
    };
    if res.into_raw() < 0 {
        return Err(MediaError::OpenXr(format!(
            "xrCreateSwapchainAndroidSurfaceKHR failed with error code {}",
            res.into_raw()
        )));
    }
    let sc = unsafe { openxr::Swapchain::<Vulkan>::from_raw(session.clone(), raw_swapchain) };
    if surface.is_null() {
        return Err(MediaError::OpenXr(
            "xrCreateSwapchainAndroidSurfaceKHR returned a null surface jobject".into(),
        ));
    }
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm() as *mut jni::sys::JavaVM) }
        .map_err(|e| MediaError::OpenXr(format!("JNI JavaVM error: {e:?}")))?;
    let (_guard, env_raw) = match vm.get_env() {
        Ok(env) => (None, env.get_raw()),
        Err(_) => {
            let guard = vm
                .attach_current_thread()
                .map_err(|e| MediaError::OpenXr(format!("JNI attach thread failed: {e:?}")))?;
            let raw = guard.get_raw();
            (Some(guard), raw)
        }
    };
    let win = unsafe { ndk_sys::ANativeWindow_fromSurface(env_raw as _, surface as _) };
    let native_window = std::ptr::NonNull::new(win)
        .map(|ptr| unsafe { ndk::native_window::NativeWindow::from_ptr(ptr) })
        .ok_or_else(|| MediaError::OpenXr("ANativeWindow_fromSurface returned NULL".into()))?;
    Ok((sc, native_window))
}
