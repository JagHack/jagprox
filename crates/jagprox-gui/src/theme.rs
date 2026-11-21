//! The launcher's look and its panes. All egui; the JagProx dark-purple palette.

use std::sync::Mutex;

use eframe::egui::{self, Color32, CursorIcon, Margin, RichText, Rounding, Stroke, Vec2};

use crate::{App, Lookup, Page};

const ACCENT: Color32 = Color32::from_rgb(150, 86, 236);
const STOP: Color32 = Color32::from_rgb(214, 76, 62);
const BG: Color32 = Color32::from_rgb(18, 18, 18);
const SURFACE: Color32 = Color32::from_rgb(30, 30, 30);
const SURFACE_HI: Color32 = Color32::from_rgb(46, 46, 46);
const WELL: Color32 = Color32::from_rgb(12, 12, 12);
const LINE: Color32 = Color32::from_rgb(48, 48, 48);
const TEXT: Color32 = Color32::from_rgb(228, 226, 232);
const MUTED: Color32 = Color32::from_rgb(150, 146, 160);

fn card_shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: egui::vec2(0.0, 3.0),
        blur: 14.0,
        spread: 0.0,
        color: Color32::from_black_alpha(110),
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.extreme_bg_color = WELL;
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgb(70, 48, 108);
    v.selection.stroke = Stroke::NONE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, LINE);
    // Plain (unstyled) buttons/inputs: surface with a subtle hover lift.
    v.widgets.inactive.weak_bg_fill = SURFACE;
    v.widgets.inactive.bg_fill = SURFACE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0f32, LINE);
    v.widgets.inactive.rounding = Rounding::same(8.0);
    v.widgets.hovered.weak_bg_fill = SURFACE_HI;
    v.widgets.hovered.bg_fill = SURFACE_HI;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0f32, ACCENT);
    v.widgets.hovered.rounding = Rounding::same(8.0);
    v.widgets.active.weak_bg_fill = SURFACE_HI;
    v.widgets.active.bg_fill = SURFACE_HI;
    v.widgets.active.rounding = Rounding::same(8.0);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 10.0);
        s.spacing.button_padding = egui::vec2(12.0, 8.0);
        s.spacing.interact_size.y = 30.0;
    });
}

/// A filled button with its own hover/press feedback and a pointer cursor.
fn button(
    ui: &mut egui::Ui,
    label: &str,
    fill: Color32,
    hover: Color32,
    text: Color32,
    size: f32,
    min: Vec2,
) -> egui::Response {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.bg_stroke = Stroke::NONE;
            w.rounding = Rounding::same(8.0);
        }
        v.widgets.inactive.weak_bg_fill = fill;
        v.widgets.inactive.bg_fill = fill;
        v.widgets.hovered.weak_bg_fill = hover;
        v.widgets.hovered.bg_fill = hover;
        v.widgets.active.weak_bg_fill = fill.gamma_multiply(0.8);
        v.widgets.active.bg_fill = fill.gamma_multiply(0.8);
        let mut b = egui::Button::new(RichText::new(label).color(text).size(size))
            .rounding(Rounding::same(8.0));
        if min != Vec2::ZERO {
            b = b.min_size(min);
        }
        let r = ui.add(b);
        if r.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        r
    })
    .inner
}

fn accent(ui: &mut egui::Ui, label: &str) -> egui::Response {
    button(
        ui,
        label,
        ACCENT,
        ACCENT.gamma_multiply(1.12),
        Color32::WHITE,
        14.0,
        Vec2::ZERO,
    )
}

pub fn sidebar(ctx: &egui::Context, app: &mut App) {
    egui::SidePanel::left("nav")
        .resizable(false)
        .exact_width(214.0)
        .frame(
            egui::Frame::none()
                .fill(SURFACE)
                .inner_margin(Margin::same(16.0)),
        )
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(egui::include_image!("../icons/icon.png"))
                        .fit_to_exact_size(egui::vec2(28.0, 28.0)),
                );
                ui.add_space(6.0);
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
    let w = ui.available_width();
    let resp = if active {
        button(
            ui,
            label,
            ACCENT,
            ACCENT.gamma_multiply(1.1),
            Color32::WHITE,
            15.0,
            egui::vec2(w, 38.0),
        )
    } else {
        button(
            ui,
            label,
            Color32::TRANSPARENT,
            SURFACE_HI,
            TEXT,
            15.0,
            egui::vec2(w, 38.0),
        )
    };
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

/// A rounded card with a border and a drop shadow.
fn card(ui: &mut egui::Ui, fill: Color32, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0f32, LINE))
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::same(18.0))
        .shadow(card_shadow())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}

pub fn aliases_page(ui: &mut egui::Ui, app: &mut App) {
    pane_title(ui, "Command Aliases");
    app.ensure_aliases_loaded();
    ui.label(
        RichText::new("Type the left command in chat and the proxy sends the right one. Both sides must start with \"/\". A running proxy picks up changes within a couple of seconds.")
            .color(MUTED)
            .size(13.0),
    );
    ui.add_space(12.0);

    let mut remove: Option<usize> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            card(ui, SURFACE, |ui| {
                if app.aliases.is_empty() {
                    ui.label(
                        RichText::new("No aliases yet — add one below.")
                            .color(MUTED)
                            .monospace(),
                    );
                }
                for (i, (from, to)) in app.aliases.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(from)
                                .hint_text("/bw4")
                                .desired_width(150.0)
                                .font(egui::TextStyle::Monospace),
                        );
                        ui.label(RichText::new("»").color(MUTED).size(16.0));
                        ui.add(
                            egui::TextEdit::singleline(to)
                                .hint_text("/play bedwars_four_four")
                                .desired_width(260.0)
                                .font(egui::TextStyle::Monospace),
                        );
                        if button(
                            ui,
                            "×",
                            STOP,
                            STOP.gamma_multiply(1.12),
                            Color32::WHITE,
                            13.0,
                            egui::vec2(30.0, 28.0),
                        )
                        .on_hover_text("Remove")
                        .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                    ui.add_space(6.0);
                }
            });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if accent(ui, "+  New Alias").clicked() {
                    app.aliases.push(("/".to_string(), "/".to_string()));
                }
                if accent(ui, "Save Aliases").clicked() {
                    app.save_aliases();
                }
                if !app.aliases_status.is_empty() {
                    ui.add_space(10.0);
                    ui.label(RichText::new(&app.aliases_status).color(ACCENT));
                }
            });
        });

    if let Some(i) = remove {
        app.aliases.remove(i);
        app.aliases_status.clear();
    }
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
            if accent(ui, "Login via Browser").clicked() {
                app.login();
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Not logged in").color(MUTED));
        }
    });

    ui.add_space((ui.available_height() * 0.22).max(16.0));
    ui.vertical_centered(|ui| {
        ui.add(
            egui::Image::new(egui::include_image!("../icons/icon.png"))
                .fit_to_exact_size(egui::vec2(104.0, 104.0)),
        );
        ui.add_space(10.0);
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
        if button(
            ui,
            label,
            fill,
            fill.gamma_multiply(1.12),
            Color32::WHITE,
            20.0,
            egui::vec2(230.0, 56.0),
        )
        .clicked()
        {
            app.toggle_proxy();
        }
    });
}

pub fn log_view(ui: &mut egui::Ui, lines: &Mutex<Vec<String>>, title: &str) {
    pane_title(ui, title);
    // The log "field": a recessed, bordered, shadowed terminal panel filling the pane.
    egui::Frame::none()
        .fill(WELL)
        .stroke(Stroke::new(1.0f32, LINE))
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::same(14.0))
        .shadow(card_shadow())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());
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
        let go = accent(ui, "Search").clicked();
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
        let go = accent(ui, "Check Status").clicked();
        if go || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            app.do_status();
        }
    });
    ui.add_space(16.0);
    result_box(ui, &app.shared.status);
}

fn result_box(ui: &mut egui::Ui, lookup: &Mutex<Lookup>) {
    let lookup = lookup.lock().unwrap_or_else(|e| e.into_inner());
    card(ui, SURFACE, |ui| match &*lookup {
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
        Lookup::Error(e) => colored_line(ui, &format!("§c{e}")),
        Lookup::Lines(lines) => {
            for line in lines {
                colored_line(ui, line);
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
                if accent(ui, "Save Settings").clicked() {
                    app.save_settings();
                }
            });

            group(ui, "Hypixel API Key", |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.api_key)
                            .password(true)
                            .hint_text(
                                if app
                                    .shared
                                    .api_key_set
                                    .load(std::sync::atomic::Ordering::SeqCst)
                                {
                                    "A key is set — enter a new one to change it"
                                } else {
                                    "Enter your Hypixel API key"
                                },
                            )
                            .desired_width(300.0),
                    );
                    if accent(ui, "Save Key").clicked() {
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
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Current version: v{}", env!("CARGO_PKG_VERSION")))
                            .color(MUTED),
                    );
                    if accent(ui, "Check for Updates").clicked() {
                        app.check_updates();
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Results appear in the Log Monitor.")
                        .color(MUTED)
                        .size(12.0),
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
                    if accent(ui, "Install Command").clicked() {
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
    card(ui, SURFACE, |ui| {
        ui.label(RichText::new(title).size(15.0).strong().color(ACCENT));
        ui.add_space(10.0);
        body(ui);
    });
    ui.add_space(14.0);
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
    jagprox_core::formatter::legacy_segments(line)
        .into_iter()
        .map(|(color, bold, text)| {
            let [r, g, b] = color.unwrap_or([210, 208, 214]);
            (Color32::from_rgb(r, g, b), bold, text)
        })
        .collect()
}
