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
        if event.state == ElementState::Released {
            renderer.application.input.key_up(format!("{code:?}"));
        }
        let context = input_context(renderer);
        let action = match event.state {
            ElementState::Pressed => renderer
                .application
                .input
                .key_down(format!("{code:?}"), context),
            ElementState::Released => None,
        };
        if let Some(action) = action {
            if event.state == ElementState::Pressed {
                match action {
                    crate::input::InputAction::ToggleFullscreen if !event.repeat => {
                        super::window_mode::toggle(window, event_loop);
                    }
                    crate::input::InputAction::ToggleConsole if !event.repeat => {
                        renderer.application.input.clear_held();
                        renderer.application.ui.console.open =
                            !renderer.application.ui.console.open;
                        renderer.application.ui.console.clear_preedit();
                        window.set_ime_allowed(renderer.application.ui.console.open);
                        let reset_cursor_position = !renderer.application.overview().is_open();
                        release_cursor(renderer, window, true, true, reset_cursor_position);
                    }
                    crate::input::InputAction::ToggleMap if !event.repeat => {
                        renderer.application.input.clear_held();
                        renderer.toggle_overview();
                        release_cursor(renderer, window, true, true, true);
                    }
                    crate::input::InputAction::Cancel if !event.repeat => {
                        if renderer.application.ui.console.open {
                            if renderer.application.ui.console.editor.is_composing() {
                                renderer.application.ui.console.clear_preedit();
                            } else {
                                renderer.application.input.clear_held();
                                renderer.application.ui.console.open = false;
                                renderer.application.ui.console.clear_preedit();
                                window.set_ime_allowed(false);
                            }
                        } else if renderer.application.overview().is_open() {
                            renderer.application.input.clear_held();
                            renderer.toggle_overview();
                        } else {
                            renderer.application.connection.cancel_transition();
                            release_cursor(renderer, window, false, false, true);
                        }
                    }
                    crate::input::InputAction::Jump if !event.repeat => {
                        renderer.input.jump_pending = true;
                    }
                    crate::input::InputAction::Interact if !event.repeat => {
                        renderer.queue_portal();
                    }
                    crate::input::InputAction::Fire if !event.repeat => {
                        if renderer.input.cursor_locked {
                            renderer.queue_fire();
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        if renderer.application.ui.console.open {
            if renderer.application.ui.console.editor.is_composing() {
                return;
            }
            if event.state == ElementState::Pressed {
                match code {
                    KeyCode::Enter => {
                        if super::controller::execute_console_input(renderer) {
                            event_loop.exit();
                        }
                    }
                    _ => super::console_events::key(
                        &mut renderer.application.ui.console,
                        code,
                        event.text.as_ref().map(|text| text.as_str()),
                        modifiers,
                        &renderer.localizer,
                        renderer.application.settings.locale,
                    ),
                }
            }
            return;
        }
        if renderer.application.overview().is_open() {
            return;
        }
    }
}

fn input_context(renderer: &Renderer) -> crate::input::InputContext {
    if renderer.application.ui.console.open {
        crate::input::InputContext::Console
    } else if renderer.application.overview().is_open() {
        crate::input::InputContext::Map
    } else {
        crate::input::InputContext::Gameplay
    }
}

pub(super) fn mouse_input(
    renderer: &mut Renderer,
    window: &Window,
    state: ElementState,
    button: MouseButton,
) {
    if state == ElementState::Released {
        renderer
            .application
            .input
            .mouse_up(format!("Mouse{button:?}"));
    }
    match (state, button) {
        (ElementState::Pressed, MouseButton::Left) => {
            if renderer.application.ui.console.open {
                if let Some((x, y)) = renderer.input.last_cursor {
                    let size = window.inner_size();
                    renderer.application.ui.console.editor.dragging =
                        renderer.application.ui.console.input_hit(
                            size.width,
                            size.height,
                            x,
                            y,
                            false,
                            &renderer.localizer,
                            renderer.application.settings.locale,
                        );
                }
                return;
            }
            if renderer.application.overview().is_open() && !renderer.application.ui.console.open {
                if renderer.overview_uses_zoom() {
                    if let Some((x, y)) = renderer.input.last_cursor {
                        let size = window.inner_size();
                        let scale = window.scale_factor();
                        let logical_width = (f64::from(size.width) / scale).round() as u32;
                        let logical_height = (f64::from(size.height) / scale).round() as u32;
                        if crate::console::slider_hit(
                            logical_width,
                            logical_height,
                            x / scale,
                            y / scale,
                        ) {
                            renderer.application.overview_begin_slider_drag();
                            renderer.set_overview_zoom(crate::console::slider_value_at_y(
                                logical_height,
                                y / scale,
                            ));
                            return;
                        }
                    }
                }
                renderer.application.overview_begin_drag();
                return;
            }
            if !renderer.application.ui.console.open && !renderer.application.overview().is_open() {
                let action = renderer.application.input.mouse_down(
                    format!("Mouse{button:?}"),
                    crate::input::InputContext::Gameplay,
                );
                if action == Some(crate::input::InputAction::Fire) && renderer.input.cursor_locked {
                    renderer.queue_fire();
                } else if action == Some(crate::input::InputAction::Fire)
                    && (window.set_cursor_grab(CursorGrabMode::Locked).is_ok()
                        || window.set_cursor_grab(CursorGrabMode::Confined).is_ok())
                {
                    window.set_cursor_visible(false);
                    renderer.input.cursor_locked = true;
                    renderer.input.last_cursor = None;
                }
            }
        }
        (ElementState::Released, MouseButton::Left) => {
            renderer.application.ui.console.editor.dragging = false;
            let was_dragged = renderer.application.overview().was_dragged();
            let slider_dragged = renderer.application.overview_end_drag();
            if renderer.application.overview().is_open()
                && !renderer.application.ui.console.open
                && !was_dragged
                && !slider_dragged
            {
                renderer.click_map_teleport();
            }
        }
        _ => {}
    }
}

pub(super) fn mouse_wheel(renderer: &mut Renderer, delta: MouseScrollDelta) {
    if renderer.application.ui.console.open {
        let amount = scroll_amount(delta, 40.0);
        renderer
            .application
            .ui
            .console
            .scroll_lines(amount.round() as isize);
    } else if renderer.application.overview().is_open() {
        let amount = scroll_amount(delta, 80.0);
        if renderer.overview_uses_zoom() {
            renderer
                .set_overview_zoom(renderer.application.overview().zoom() * (1.0 - amount * 0.1));
        }
    }
}

pub(super) fn cursor_moved(
    renderer: &mut Renderer,
    window: &Window,
    position: PhysicalPosition<f64>,
) {
    if renderer.application.overview().is_slider_dragging() {
        let scale = window.scale_factor();
        renderer.set_overview_zoom(crate::console::slider_value_at_y(
            (f64::from(window.inner_size().height) / scale).round() as u32,
            position.y / scale,
        ));
    }
    if renderer.application.ui.console.open && renderer.application.ui.console.editor.dragging {
        let size = window.inner_size();
        renderer.application.ui.console.input_hit(
            size.width,
            size.height,
            position.x,
            position.y,
            true,
            &renderer.localizer,
            renderer.application.settings.locale,
        );
    }
    if renderer.application.overview().is_open()
        && renderer.application.overview().is_dragging()
        && !renderer.application.ui.console.open
    {
        if let Some((x, y)) = renderer.input.last_cursor {
            let horizontal = if renderer.is_earth_world() { 1.0 } else { -1.0 };
            renderer
                .application
                .overview_drag_by(position.x - x, position.y - y, horizontal);
        }
    }
    if renderer.input.cursor_locked
        && !renderer.application.ui.console.open
        && !renderer.application.overview().is_open()
    {
        if let Some((x, y)) = renderer.input.last_cursor {
            let horizontal = if renderer.is_earth_world() { -1.0 } else { 1.0 };
            renderer.input.yaw +=
                ((position.x - x) as f32) * renderer.mouse_sensitivity * horizontal;
            renderer.input.pitch -= ((position.y - y) as f32) * renderer.mouse_sensitivity;
            renderer.input.pitch = renderer.input.pitch.clamp(-1.5, 1.5);
        }
    }
    renderer.input.last_cursor = Some((position.x, position.y));
}

pub(super) fn device_event(renderer: &mut Renderer, event: DeviceEvent) {
    if let DeviceEvent::MouseMotion { delta } = event {
        if renderer.input.cursor_locked
            && !renderer.application.ui.console.open
            && !renderer.application.overview().is_open()
        {
            let horizontal = if renderer.is_earth_world() { -1.0 } else { 1.0 };
            renderer.input.yaw += (delta.0 as f32) * renderer.mouse_sensitivity * horizontal;
            renderer.input.pitch -= (delta.1 as f32) * renderer.mouse_sensitivity;
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
        renderer.application.input.clear_held();
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
