use crate::console::Console;
use winit::keyboard::{KeyCode, ModifiersState};

pub(super) fn key(
    console: &mut Console,
    code: KeyCode,
    text: Option<&str>,
    modifiers: ModifiersState,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) {
    let shift = modifiers.shift_key();
    let ctrl = modifiers.control_key() && !modifiers.alt_key();
    if ctrl {
        let editor = &mut console.editor;
        match code {
            KeyCode::KeyA => editor.select_all(),
            KeyCode::KeyC => clipboard_write(console, false, localizer, locale),
            KeyCode::KeyX => clipboard_write(console, true, localizer, locale),
            KeyCode::KeyV => match arboard::Clipboard::new().and_then(|mut board| board.get_text())
            {
                Ok(value) => console.append_text(&value, localizer, locale),
                Err(_error) => {
                    console.push_ui_error_message(
                        minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::ClipboardUnavailable,
                        ),
                        localizer,
                        locale,
                    );
                }
            },
            KeyCode::KeyZ if shift => {
                editor.redo();
                console.edit_changed(localizer, locale);
            }
            KeyCode::KeyZ => {
                editor.undo();
                console.edit_changed(localizer, locale);
            }
            KeyCode::KeyY => {
                editor.redo();
                console.edit_changed(localizer, locale);
            }
            KeyCode::ArrowLeft => editor.left(shift, true),
            KeyCode::ArrowRight => editor.right(shift, true),
            KeyCode::Backspace => {
                editor.delete(true, true);
                console.edit_changed(localizer, locale);
            }
            KeyCode::Delete => {
                editor.delete(false, true);
                console.edit_changed(localizer, locale);
            }
            _ => {}
        }
        if matches!(
            code,
            KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::KeyA
        ) {
            console.refresh_suggestions(localizer, locale);
        }
        return;
    }
    match code {
        KeyCode::ArrowLeft => console.editor.left(shift, false),
        KeyCode::ArrowRight => console.editor.right(shift, false),
        KeyCode::Home => console.editor.move_to(0, shift),
        KeyCode::End => console.editor.move_to(console.editor.text.len(), shift),
        KeyCode::Backspace => console.backspace(localizer, locale),
        KeyCode::Delete => {
            console.editor.delete(false, false);
            console.edit_changed(localizer, locale);
        }
        KeyCode::ArrowUp => console.history_up(localizer, locale),
        KeyCode::ArrowDown => console.history_down(localizer, locale),
        KeyCode::PageUp => console.scroll_lines(10),
        KeyCode::PageDown => console.scroll_lines(-10),
        KeyCode::Tab => console.complete_input(shift, localizer, locale),
        _ => {
            if let Some(value) = text {
                console.append_text(value, localizer, locale);
            }
        }
    }
    if matches!(
        code,
        KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::Home | KeyCode::End
    ) {
        console.refresh_suggestions(localizer, locale);
    }
}

fn clipboard_write(
    console: &mut Console,
    cut: bool,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) {
    let selected = console.editor.selected_text().to_owned();
    if selected.is_empty() {
        return;
    }
    match arboard::Clipboard::new().and_then(|mut board| board.set_text(selected)) {
        Ok(()) => {
            if cut {
                console.editor.replace_selection("");
                console.edit_changed(localizer, locale);
            }
        }
        Err(_error) => {
            console.push_ui_error_message(
                minimium_localization::UiMessage::Notice(
                    minimium_localization::UiNotice::ClipboardUnavailable,
                ),
                localizer,
                locale,
            );
        }
    }
}
