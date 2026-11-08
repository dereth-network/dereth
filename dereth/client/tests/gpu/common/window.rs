//! One invisible native window for the tests that need a real swap chain.
//!
//! A process may make only one event loop. On Windows the window and its loop live on a thread of
//! their own, made on first use and kept until the process ends, so every test of the process
//! draws into the same window; its handle is carried to the tests as the number it is. Elsewhere
//! the window is made on the calling test's thread, which may happen once a process; on Linux it
//! opens on the display the run inherits, so a machine with none runs it under Xvfb. A Mac makes an
//! event loop only on the process's main thread, which no test runs on.

use dereth_render::device::WindowHandles;
use winit::dpi::PhysicalSize;

/// Makes one invisible window when the loop is running.
struct Invisible(Option<winit::window::Window>);

impl winit::application::ApplicationHandler for Invisible {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.0.is_some() {
            return;
        }
        let attributes = winit::window::Window::default_attributes()
            .with_visible(false)
            .with_inner_size(PhysicalSize::new(800, 600));
        self.0 = Some(
            event_loop
                .create_window(attributes)
                .expect("an invisible native window"),
        );
    }
    fn window_event(
        &mut self,
        _: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        _: winit::event::WindowEvent,
    ) {
    }
}

/// An event loop on this thread, with its invisible 800x600 window made.
fn make() -> (winit::event_loop::EventLoop<()>, Invisible) {
    use winit::platform::pump_events::EventLoopExtPumpEvents as _;
    let mut builder = winit::event_loop::EventLoop::builder();
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        builder.with_any_thread(true);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoopBuilderExtX11::with_any_thread(&mut builder, true);
        EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
    }
    let mut event_loop = builder.build().expect("an event loop");
    let mut invisible = Invisible(None);
    event_loop.pump_app_events(Some(std::time::Duration::ZERO), &mut invisible);
    assert!(invisible.0.is_some(), "the running loop made the window");
    (event_loop, invisible)
}

/// The handles of `invisible`'s window.
fn handles_of(invisible: &Invisible) -> WindowHandles {
    let window = invisible.0.as_ref().expect("the window");
    dereth_desktop::platform::window::window_handles(window).expect("window handles")
}

/// The process's invisible 800x600 window, made on a thread of its own on first use and kept until
/// the process ends.
///
/// # Panics
/// When the platform makes no event loop or window here.
#[cfg(windows)]
pub fn invisible_window() -> WindowHandles {
    use std::num::NonZeroIsize;
    use std::sync::OnceLock;
    use winit::raw_window_handle::{
        RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
    };
    // The window's handle and its module's, as numbers.
    static WINDOW: OnceLock<(isize, Option<isize>)> = OnceLock::new();
    let (hwnd, hinstance) = *WINDOW.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use winit::platform::pump_events::EventLoopExtPumpEvents as _;
            let (mut event_loop, mut invisible) = make();
            let RawWindowHandle::Win32(h) = handles_of(&invisible).window else {
                panic!("a Windows window has a Win32 handle");
            };
            tx.send((h.hwnd.get(), h.hinstance.map(NonZeroIsize::get)))
                .expect("the test waits for the window");
            loop {
                event_loop
                    .pump_app_events(Some(std::time::Duration::from_millis(50)), &mut invisible);
            }
        });
        rx.recv().expect("the window's thread made a window")
    });
    let mut window = Win32WindowHandle::new(NonZeroIsize::new(hwnd).expect("a window handle"));
    window.hinstance = hinstance.and_then(NonZeroIsize::new);
    WindowHandles {
        display: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
        window: RawWindowHandle::Win32(window),
    }
}

/// An invisible 800x600 window made on this thread, kept until the process ends.
///
/// # Panics
/// When the platform makes no event loop or window here.
#[cfg(not(windows))]
pub fn invisible_window() -> WindowHandles {
    let (event_loop, invisible) = make();
    let handles = handles_of(&invisible);
    std::mem::forget(invisible);
    std::mem::forget(event_loop);
    handles
}
