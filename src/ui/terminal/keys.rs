//! The application shortcuts handled by the terminal.

use super::{Command, Dialog, Terminal};
use egui::{Key, Modifiers};

impl Terminal {
    pub(super) fn handle_keys(&self, ctx: &egui::Context, commands: &mut Vec<Command>) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        ctx.input_mut(|input| {
            for (modifiers, key, command) in [
                (Modifiers::NONE, Key::F1, Command::Open(Dialog::Shortcuts)),
                (
                    Modifiers::SHIFT,
                    Key::Slash,
                    Command::Open(Dialog::Shortcuts),
                ),
                (
                    Modifiers::NONE,
                    Key::Questionmark,
                    Command::Open(Dialog::Shortcuts),
                ),
                (Modifiers::COMMAND | Modifiers::SHIFT, Key::Z, Command::Redo),
                (Modifiers::COMMAND, Key::Z, Command::Undo),
                (Modifiers::COMMAND, Key::Y, Command::Redo),
            ] {
                if input.consume_key(modifiers, key) {
                    commands.push(command);
                }
            }
        });
    }
}
