//! The launcher's look and its panes. All egui; the JagProx dark-purple palette.

use std::sync::Mutex;

use eframe::egui::{self, Color32, RichText, Rounding, Stroke};

use crate::{App, Lookup, Page};

const ACCENT: Color32 = Color32::from_rgb(150, 86, 236);
const ACCENT_DIM: Color32 = Color32::from_rgb(70, 48, 108);
const STOP: Color32 = Color32::from_rgb(214, 76, 62);
const BG: Color32 = Color32::from_rgb(18, 18, 18);
const SURFACE: Color32 = Color32::from_rgb(30, 30, 30);
const SURFACE_HI: Color32 = Color32::from_rgb(44, 44, 44);
const LINE: Color32 = Color32::from_rgb(48, 48, 48);
const TEXT: Color32 = Color32::from_rgb(228, 226, 232);
const MUTED: Color32 = Color32::from_rgb(150, 146, 160);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.extreme_bg_color = Color32::from_rgb(12, 12, 12);
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::NONE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, LINE);
    v.widgets.inactive.bg_fill = SURFACE;
    v.widgets.inactive.weak_bg_fill = SURFACE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0f32, LINE);
    v.widgets.inactive.rounding = Rounding::same(8.0);
    v.widgets.hovered.bg_fill = SURFACE_HI;
    v.widgets.hovered.weak_bg_fill = SURFACE_HI;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0f32, ACCENT_DIM);
    v.widgets.hovered.rounding = Rounding::same(8.0);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.active.rounding = Rounding::same(8.0);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 10.0);
        s.spacing.button_padding = egui::vec2(12.0, 8.0);
        s.spacing.interact_size.y = 30.0;
    });
}

pub fn sidebar(ctx: &egui::Context, app: &mut App) {
    egui::SidePanel::left("nav")
        .resizable(false)
        .exact_width(214.0)
        .frame(
            egui::Frame::none()
                .fill(SURFACE)
                .inner_margin(egui::Margin::same(16.0)),
        )
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚡").size(22.0).color(ACCENT));
                ui.label(RichText::new("JagProx").size(22.0).strong().color(TEXT));
            });
            ui.add_space(20.0);
            for (page, label) in [
                (Page::Home, "Home"),
                (Page::Logs, "Log Monitor"),
                (Page::Chat, "In-Game Chat"),
                (Page::Aliases, "Aliases"),
                (Page::Stats, "Stat Search"),
                (Page::Status, "Status Check"),
                (Page::Settings, "Settings"),
            ] {
                nav_item(ui, app, page, label);
                ui.add_space(4.0);
            }
        });
}

fn nav_item(ui: &mut egui::Ui, app: &mut App, page: Page, label: &str) {
    let active = app.page == page;
    let text = RichText::new(label)
        .size(15.0)
        .color(if active { Color32::WHITE } else { TEXT });
    let w = ui.available_width();
    let resp = ui.add_sized(
        [w, 38.0],
        egui::Button::new(text)
            .fill(if active { ACCENT } else { Color32::TRANSPARENT })
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(8.0)),
    );
    if resp.clicked() {
        app.page = page;
    }
}

/// A heading + a thin rule, used at the top of each pane.
fn pane_title(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).size(22.0).strong());
    ui.add_space(6.0);
    let rect = ui.max_rect();
    let y = ui.cursor().top();
    ui.painter()
        .hline(rect.x_range(), y, Stroke::new(1.0f32, LINE));
    ui.add_space(14.0);
}

pub fn placeholder(ui: &mut egui::Ui, title: &str, msg: &str) {
    pane_title(ui, title);
    ui.label(RichText::new(msg).color(MUTED));
}

pub fn home(ui: &mut egui::Ui, app: &mut App) {
    let name = app.shared.display_name();
    let logging_in = app
        .shared
        .logging_in
        .load(std::sync::atomic::Ordering::SeqCst);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        if let Some(name) = &name {
            if ui.button("Logout").clicked() {
                app.logout();
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("Logged in as: {name}"))
                    .color(ACCENT)
                    .strong(),
            );
        } else if logging_in {
            ui.label(RichText::new("Waiting for browser login…").color(MUTED));
        } else {
            if ui.button("Login via Browser").clicked() {
                app.login();
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Not logged in").color(MUTED));
        }
    });

    ui.add_space((ui.available_height() * 0.32).max(20.0));
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("JagProx").size(58.0).strong().color(ACCENT));
        ui.label(
            RichText::new("Your Hypixel Advantage")
                .size(18.0)
                .color(MUTED),
        );
        ui.add_space(26.0);
        let running = app
            .shared
            .proxy_running
            .load(std::sync::atomic::Ordering::SeqCst);
        let (label, fill) = if running {
            ("Stop Proxy", STOP)
        } else {
            ("Launch Proxy", ACCENT)
        };
        let btn = egui::Button::new(
            RichText::new(label)
                .size(20.0)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(fill)
        .rounding(Rounding::same(10.0))
        .min_size(egui::vec2(230.0, 56.0));
        if ui.add(btn).clicked() {
            app.toggle_proxy();
        }
    });
}

pub fn log_view(ui: &mut egui::Ui, lines: &Mutex<Vec<String>>, title: &str) {
    pane_title(ui, title);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let lines = lines.lock().unwrap_or_else(|e| e.into_inner());
            if lines.is_empty() {
                ui.label(RichText::new("Nothing yet.").color(MUTED).monospace());
            }
            for line in lines.iter() {
                colored_line(ui, line);
            }
        });
}

pub fn stats_page(ui: &mut egui::Ui, app: &mut App) {
    pane_title(ui, "Player Stat Search");
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.stat_name)
                .hint_text("Player name")
                .desired_width(220.0),
        );
        let current = app
            .gamemodes
            .iter()
            .find(|(_, v)| *v == app.stat_mode)
            .map(|(n, _)| n.clone())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("mode")
            .selected_text(current)
            .width(180.0)
            .show_ui(ui, |ui| {
                for (name, value) in &app.gamemodes {
                    ui.selectable_value(&mut app.stat_mode, value.clone(), name);
                }
            });
        let go = ui.add(accent_button("Search")).clicked();
        if go || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            app.do_stats();
        }
    });
    ui.add_space(16.0);
    result_box(ui, &app.shared.stats);
}

pub fn status_page(ui: &mut egui::Ui, app: &mut App) {
    pane_title(ui, "Player Status Check");
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.status_name)
                .hint_text("Player name")
                .desired_width(220.0),
        );
        let go = ui.add(accent_button("Check Status")).clicked();
        if go || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            app.do_status();
        }
    });
    ui.add_space(16.0);
    result_box(ui, &app.shared.status);
}

fn result_box(ui: &mut egui::Ui, lookup: &Mutex<Lookup>) {
    let lookup = lookup.lock().unwrap_or_else(|e| e.into_inner());
    egui::Frame::none()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0f32, LINE))
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &*lookup {
                Lookup::Idle => {
                    ui.label(
                        RichText::new("Enter a player and search.")
                            .color(MUTED)
                            .monospace(),
                    );
                }
                Lookup::Loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Looking up…").color(MUTED));
                    });
                }
                Lookup::Error(e) => {
                    colored_line(ui, &format!("§c{e}"));
                }
                Lookup::Lines(lines) => {
                    for line in lines {
                        colored_line(ui, line);
                    }
                }
            }
        });
}

pub fn settings(ui: &mut egui::Ui, app: &mut App) {
    pane_title(ui, "Settings");
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            group(ui, "Auto GG", |ui| {
                ui.checkbox(
                    &mut app.autogg_enabled,
                    "Send \"gg\" automatically at the end of a game",
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Message");
                    ui.add(egui::TextEdit::singleline(&mut app.autogg_msg).desired_width(160.0));
                    ui.add_space(12.0);
                    ui.label("Delay (ms)");
                    ui.add(egui::TextEdit::singleline(&mut app.autogg_delay).desired_width(80.0));
                });
                ui.add_space(10.0);
                if ui.add(accent_button("Save Settings")).clicked() {
                    app.save_settings();
                }
            });

            group(ui, "Hypixel API Key", |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.api_key)
                            .password(true)
                            .hint_text(if app.api_key_set {
                                "A key is set — enter a new one to change it"
                            } else {
                                "Enter your Hypixel API key"
                            })
                            .desired_width(300.0),
                    );
                    if ui.add(accent_button("Save Key")).clicked() {
                        app.save_api_key();
                    }
                });
            });

            group(ui, "Discord Rich Presence", |ui| {
                if ui
                    .checkbox(&mut app.discord_rpc, "Show JagProx in your Discord status")
                    .changed()
                {
                    app.toggle_discord(app.discord_rpc);
                }
            });

            group(ui, "Updates", |ui| {
                ui.label(
                    RichText::new(format!("Current version: v{}", env!("CARGO_PKG_VERSION")))
                        .color(MUTED),
                );
            });

            let kind = jagprox_core::install::detect();
            if matches!(
                kind,
                jagprox_core::install::Install::AppImage { .. }
                    | jagprox_core::install::Install::MacApp { .. }
            ) {
                group(ui, "Command-line Tool", |ui| {
                    ui.label(RichText::new("Use jagprox from any terminal.").color(MUTED));
                    ui.add_space(8.0);
                    if ui.add(accent_button("Install Command")).clicked() {
                        app.install_cli_clicked();
                    }
                    if !app.cli_status.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new(&app.cli_status).color(MUTED));
                    }
                });
            }
        });
}

fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0f32, LINE))
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(15.0).strong().color(ACCENT));
            ui.add_space(10.0);
            body(ui);
        });
    ui.add_space(14.0);
}

fn accent_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(label.to_string())
            .color(Color32::WHITE)
            .strong(),
    )
    .fill(ACCENT)
    .rounding(Rounding::same(8.0))
}

fn colored_line(ui: &mut egui::Ui, line: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (color, bold, text) in segments(line) {
            let mut rt = RichText::new(text).monospace().color(color).size(13.0);
            if bold {
                rt = rt.strong();
            }
            ui.label(rt);
        }
    });
}

fn segments(line: &str) -> Vec<(Color32, bool, String)> {
    let default = Color32::from_rgb(210, 208, 214);
    let mut out = Vec::new();
    let mut color = default;
    let mut bold = false;
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '§'
            && let Some(&code) = chars.peek()
        {
            chars.next();
            if !cur.is_empty() {
                out.push((color, bold, std::mem::take(&mut cur)));
            }
            if let Some(c) = code_color(code) {
                color = c;
                bold = false;
            } else if code == 'l' {
                bold = true;
            } else if code == 'r' {
                color = default;
                bold = false;
            }
            // italics/underline/strike/obfuscate are ignored.
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push((color, bold, cur));
    }
    out
}

fn code_color(c: char) -> Option<Color32> {
    Some(match c {
        '0' => Color32::from_rgb(0, 0, 0),
        '1' => Color32::from_rgb(0, 0, 170),
        '2' => Color32::from_rgb(0, 170, 0),
        '3' => Color32::from_rgb(0, 170, 170),
        '4' => Color32::from_rgb(200, 40, 40),
        '5' => Color32::from_rgb(170, 0, 170),
        '6' => Color32::from_rgb(255, 170, 0),
        '7' => Color32::from_rgb(170, 170, 170),
        '8' => Color32::from_rgb(85, 85, 85),
        '9' => Color32::from_rgb(85, 85, 255),
        'a' => Color32::from_rgb(85, 255, 85),
        'b' => Color32::from_rgb(85, 255, 255),
        'c' => Color32::from_rgb(255, 85, 85),
        'd' => Color32::from_rgb(255, 85, 255),
        'e' => Color32::from_rgb(255, 255, 85),
        'f' => Color32::from_rgb(255, 255, 255),
        _ => return None,
    })
}
