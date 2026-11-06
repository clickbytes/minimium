use super::*;
use winit::{
    dpi::PhysicalPosition,
    event::{KeyEvent, MouseScrollDelta},
};

pub(super) fn keyboard(
    renderer: &mut Renderer,
    window: &Window,
    event_loop: &ActiveEventLoop,
    event: KeyEvent,
    modifiers: winit::keyboard::ModifiersState,
) {
    if let PhysicalKey::Code(code) = event.physical_key {
        if code == KeyCode::F11 && event.state == ElementState::Pressed && !event.repeat {
            super::window_mode::toggle(window, event_loop);
            return;
        }
        if code == KeyCode::F1 && event.state == ElementState::Pressed && !event.repeat {
            renderer.ui.console.open = !renderer.ui.console.open;
            let reset_cursor_position = !renderer.overview_open;
            release_cursor(renderer, window, true, true, reset_cursor_position);
            return;
        }
        if renderer.ui.console.open {
            if event.state == ElementState::Pressed {
                match code {
                    KeyCode::Escape => renderer.ui.console.open = false,
                    KeyCode::Enter => {
                        if super::controller::execute_console_input(renderer) {
                            event_loop.exit();
                        }
                    }
                    _ => super::console_events::key(
                        &mut renderer.ui.console,
                        code,
                        event.text.as_ref().map(|text| text.as_str()),
                        modifiers,
                    ),
                }
            }
            return;
        }
        if code == KeyCode::KeyM && event.state == ElementState::Pressed && !event.repeat {
            renderer.toggle_overview();
            release_cursor(renderer, window, true, true, true);
            return;
        }
        if renderer.overview_open {
            if code == KeyCode::Escape && event.state == ElementState::Pressed {
                renderer.toggle_overview();
            }
            return;
        }
        if code == KeyCode::Escape && event.state == ElementState::Pressed {
            renderer.connection.cancel_transition();
            release_cursor(renderer, window, false, false, true);
        } else if event.state == ElementState::Pressed {
            renderer.input.keys.insert(code);
            if code == KeyCode::Space && !event.repeat {
                renderer.input.jump_pending = true;
            }
            if code == KeyCode::KeyE && !event.repeat {
                renderer.queue_portal();
            }
        } else {
            renderer.input.keys.remove(&code);
        }
    }
}

pub(super) fn mouse_input(
    renderer: &mut Renderer,
    window: &Window,
    state: ElementState,
    button: MouseButton,
) {
    match (state, button) {
        (ElementState::Pressed, MouseButton::Left) => {
            if renderer.ui.console.open {
                if let Some((x, y)) = renderer.input.last_cursor {
                    let size = window.inner_size();
                    renderer.ui.console.editor.dragging =
                        renderer
                            .ui
                            .console
                            .input_hit(size.width, size.height, x, y, false);
                }
                return;
            }
            if renderer.overview_open && !renderer.ui.console.open {
                if renderer.overview_uses_zoom() {
                    if let Some((x, y)) = renderer.input.last_cursor {
                        let size = window.inner_size();
                        if crate::console::slider_hit(size.width, size.height, x, y) {
                            renderer.overview_slider_dragging = true;
                            renderer.overview_dragged = true;
                            renderer.set_overview_zoom(crate::console::slider_value_at_y(
                                size.height,
                                y,
                            ));
                            return;
                        }
                    }
                }
                renderer.overview_dragging = true;
                renderer.overview_dragged = false;
                renderer.overview_drag_distance = 0.0;
                return;
            }
            if !renderer.ui.console.open && !renderer.overview_open {
                if renderer.input.cursor_locked {
                    renderer.queue_fire();
                } else if window.set_cursor_grab(CursorGrabMode::Locked).is_ok()
                    || window.set_cursor_grab(CursorGrabMode::Confined).is_ok()
                {
                    window.set_cursor_visible(false);
                    renderer.input.cursor_locked = true;
                    renderer.input.last_cursor = None;
                }
            }
        }
        (ElementState::Released, MouseButton::Left) => {
            renderer.ui.console.editor.dragging = false;
            let slider_dragged = std::mem::take(&mut renderer.overview_slider_dragging);
            if renderer.overview_open
                && !renderer.ui.console.open
                && !renderer.overview_dragged
                && !slider_dragged
            {
                renderer.click_map_teleport();
            }
            renderer.overview_dragging = false;
        }
        _ => {}
    }
}

pub(super) fn mouse_wheel(renderer: &mut Renderer, delta: MouseScrollDelta) {
    if renderer.ui.console.open {
        let amount = scroll_amount(delta, 40.0);
        renderer.ui.console.scroll_lines(amount.round() as isize);
    } else if renderer.overview_open {
        let amount = scroll_amount(delta, 80.0);
        if renderer.overview_uses_zoom() {
            renderer.set_overview_zoom(renderer.overview_zoom * (1.0 - amount * 0.1));
        }
    }
}

pub(super) fn cursor_moved(
    renderer: &mut Renderer,
    window: &Window,
    position: PhysicalPosition<f64>,
) {
    if renderer.overview_slider_dragging {
        renderer.set_overview_zoom(crate::console::slider_value_at_y(
            window.inner_size().height,
            position.y,
        ));
    }
    if renderer.ui.console.open && renderer.ui.console.editor.dragging {
        let size = window.inner_size();
        renderer
            .ui
            .console
            .input_hit(size.width, size.height, position.x, position.y, true);
    }
    if renderer.overview_open && renderer.overview_dragging && !renderer.ui.console.open {
        if let Some((x, y)) = renderer.input.last_cursor {
            renderer.overview_drag_distance +=
                ((position.x - x).powi(2) + (position.y - y).powi(2)).sqrt();
            renderer.overview_dragged |= renderer.overview_drag_distance > 5.0;
            let horizontal = if renderer.is_earth_world() { 1.0 } else { -1.0 };
            renderer.overview_yaw += (position.x - x) as f32 * 0.005 * horizontal;
            renderer.overview_pitch =
                (renderer.overview_pitch + (position.y - y) as f32 * 0.005).clamp(-1.4, 1.4);
        }
    }
    if renderer.input.cursor_locked && !renderer.ui.console.open && !renderer.overview_open {
        if let Some((x, y)) = renderer.input.last_cursor {
            let horizontal = if renderer.is_earth_world() { -1.0 } else { 1.0 };
            renderer.input.yaw += ((position.x - x) as f32) * 0.0025 * horizontal;
            renderer.input.pitch -= ((position.y - y) as f32) * 0.0025;
            renderer.input.pitch = renderer.input.pitch.clamp(-1.5, 1.5);
        }
    }
    renderer.input.last_cursor = Some((position.x, position.y));
}

pub(super) fn device_event(renderer: &mut Renderer, event: DeviceEvent) {
    if let DeviceEvent::MouseMotion { delta } = event {
        if renderer.input.cursor_locked && !renderer.ui.console.open && !renderer.overview_open {
            let horizontal = if renderer.is_earth_world() { -1.0 } else { 1.0 };
            renderer.input.yaw += (delta.0 as f32) * 0.0025 * horizontal;
            renderer.input.pitch -= (delta.1 as f32) * 0.0025;
            renderer.input.pitch = renderer.input.pitch.clamp(-1.5, 1.5);
        }
    }
}

fn release_cursor(
    renderer: &mut Renderer,
    window: &Window,
    clear_keys: bool,
    clear_jump: bool,
    reset_cursor_position: bool,
) {
    let _ = window.set_cursor_grab(CursorGrabMode::None);
    window.set_cursor_visible(true);
    renderer.input.cursor_locked = false;
    if reset_cursor_position {
        renderer.input.last_cursor = None;
    }
    if clear_keys {
        renderer.input.keys.clear();
    }
    if clear_jump {
        renderer.input.jump_pending = false;
    }
}

fn scroll_amount(delta: MouseScrollDelta, pixel_scale: f32) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, lines) => lines,
        MouseScrollDelta::PixelDelta(position) => position.y as f32 / pixel_scale,
    }
}
