//! Desktop display mode policy; dimensions are physical screen pixels.
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::{Fullscreen, Window, WindowAttributes},
};
pub(super) fn attributes(event_loop: &ActiveEventLoop, windowed: bool) -> WindowAttributes {
    let monitor = event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next());
    let size = monitor
        .as_ref()
        .map_or(PhysicalSize::new(1280, 720), |m| m.size());
    WindowAttributes::default()
        .with_position(PhysicalPosition::new(0, 0))
        .with_inner_size(size)
        .with_fullscreen(if windowed {
            None
        } else {
            Some(Fullscreen::Borderless(monitor))
        })
}
pub(super) fn fit_windowed(window: &Window, event_loop: &ActiveEventLoop) {
    window.set_outer_position(PhysicalPosition::new(0, 0));
    let Some(monitor) = event_loop
        .primary_monitor()
        .or_else(|| window.current_monitor())
    else {
        return;
    };
    let screen = monitor.size();
    let outer = window.outer_size();
    let inner = window.inner_size();
    // Account for the title bar and borders so the outer frame fits the screen.
    let size = PhysicalSize::new(
        screen
            .width
            .saturating_sub(outer.width.saturating_sub(inner.width))
            .max(1),
        screen
            .height
            .saturating_sub(outer.height.saturating_sub(inner.height))
            .max(1),
    );
    let _ = window.request_inner_size(size);
    window.set_outer_position(PhysicalPosition::new(0, 0));
}
pub(super) fn toggle(window: &Window, event_loop: &ActiveEventLoop) {
    if window.fullscreen().is_some() {
        window.set_fullscreen(None);
        fit_windowed(window, event_loop);
    } else {
        window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
    }
}
