//! Benchmark-only instrumentation, never linked into the production index.
use super::Case;

pub(super) struct Window {
    allocations: bool,
    #[cfg(all(feature = "bench-profile", target_os = "linux"))]
    stop_profile: Option<unsafe extern "C" fn()>,
}
impl Window {
    pub(super) fn start() -> Self {
        let allocations = mudu_sys::env_var::var("MUDU_BENCH_ALLOCATIONS").is_some();
        if allocations {
            #[cfg(feature = "bench-alloc")]
            super::allocations::start();
            #[cfg(not(feature = "bench-alloc"))]
            panic!("allocation statistics require the bench-alloc feature");
        }
        let path = mudu_sys::env_var::var("MUDU_BENCH_PROFILE");
        #[cfg(all(feature = "bench-profile", target_os = "linux"))]
        let stop_profile = path.map(|path| {
            let path = std::ffi::CString::new(path).expect("profile path contains NUL");
            // SAFETY: Names and signatures are the documented gperftools C API.
            // The preloaded library remains mapped for the executable's lifetime.
            unsafe {
                let start = libc::dlsym(libc::RTLD_DEFAULT, c"ProfilerStart".as_ptr());
                let stop = libc::dlsym(libc::RTLD_DEFAULT, c"ProfilerStop".as_ptr());
                assert!(!start.is_null() && !stop.is_null(), "preload libprofiler");
                let start: unsafe extern "C" fn(*const libc::c_char) -> libc::c_int =
                    std::mem::transmute(start);
                assert_ne!(start(path.as_ptr()), 0, "ProfilerStart failed");
                std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn()>(stop)
            }
        });
        #[cfg(not(all(feature = "bench-profile", target_os = "linux")))]
        assert!(
            path.is_none(),
            "CPU profiling requires Linux and bench-profile"
        );
        Self {
            allocations,
            #[cfg(all(feature = "bench-profile", target_os = "linux"))]
            stop_profile,
        }
    }
    pub(super) fn finish(mut self, case: &Case, paged: bool, trial: usize) {
        self.stop_profile();
        if self.allocations {
            #[cfg(feature = "bench-alloc")]
            {
                let (calls, bytes, reallocs) = super::allocations::stop();
                println!(
                    "ALLOC,{},{},{},{},{},{}",
                    case.name,
                    if paged { "paged" } else { "baseline" },
                    trial,
                    calls,
                    bytes,
                    reallocs
                );
            }
            self.allocations = false;
        }
        let _ = (case, paged, trial);
    }
    fn stop_profile(&mut self) {
        #[cfg(all(feature = "bench-profile", target_os = "linux"))]
        if let Some(stop) = self.stop_profile.take() {
            // SAFETY: Resolved from the preloaded gperftools library in start().
            unsafe { stop() };
        }
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        self.stop_profile();
        #[cfg(feature = "bench-alloc")]
        if self.allocations {
            super::allocations::stop();
        }
    }
}
