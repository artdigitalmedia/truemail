//! Replies ready to change or send, beside Reply: Canned text (replies kept by name, picked
//! from a menu) and Draft a reply (one written by a language model on this computer). Either
//! starts a reply with the text in it; nothing is sent until the user sends it.

use egui::{RichText, Ui};
use mailcraft_engine::Session;
use serde_json::json;

use crate::dialogs::{CannedForm, Dialog};
use crate::icons::Icon;
use crate::theme::Tokens;
use crate::{Action, widgets};

/// The Canned text button: a menu of the replies kept, then making one and changing one.
pub(crate) fn canned_button(ui: &mut Ui, s: &Session, t: &Tokens, acts: &mut Vec<Action>) {
    let b = widgets::pill_button(ui, Icon::Note, "Canned text", t).on_hover_text(tl!("Reply with a text you keep for answers you give often"));
    egui::Popup::menu(&b).show(|ui| {
        if s.prefs.canned.is_empty() {
            ui.label(RichText::new(tl!("No canned text yet.")).weak());
        }
        for (i, c) in s.prefs.canned.iter().enumerate() {
            // What it says, when pointed at: the name alone doesn't always tell two apart.
            let preview: String = c.text.chars().take(300).collect();
            if ui.button(c.name.as_str()).on_hover_text(preview).clicked() {
                acts.push(Action::Run("mail.cannedReply", json!({"index": i})));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tl!("New canned text…")).clicked() {
            acts.push(Action::Dialog(Dialog::CannedText(Box::new(CannedForm::new(s)))));
            ui.close();
        }
        if !s.prefs.canned.is_empty() {
            ui.menu_button(tl!("Edit a canned text"), |ui| {
                for c in &s.prefs.canned {
                    if ui.button(c.name.as_str()).clicked() {
                        acts.push(Action::Dialog(Dialog::CannedText(Box::new(CannedForm::edit(s, c)))));
                        ui.close();
                    }
                }
            });
        }
    });
}

/// The Draft a reply button. While a draft is being written it says so and does nothing: the
/// model writes one at a time, and it takes a while.
pub(crate) fn draft_button(ui: &mut Ui, s: &Session, t: &Tokens, acts: &mut Vec<Action>) {
    if s.drafting.is_some() {
        ui.add_enabled_ui(false, |ui| {
            widgets::pill_button(ui, Icon::Clock, "Drafting…", t)
                .on_disabled_hover_text(tl!("The reply opens when it's written; this can take a minute"));
        });
        return;
    }
    let b = widgets::pill_button(ui, Icon::Star, "Draft a reply", t)
        .on_hover_text(tl!("A reply written on this computer for you to change or send; the message goes nowhere else"));
    if b.clicked() {
        acts.push(Action::Run("mail.draftReply", json!({})));
    }
}
