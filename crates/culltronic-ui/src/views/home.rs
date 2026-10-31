//! Centered launch screen with folder actions and recent sessions.

use super::Action;
use crate::theme;
use culltronic_core::Db;
use eframe::egui::{self, Align2, Color32, FontId, Stroke, pos2, vec2};
use std::path::Path;

const BACKGROUND: Color32 = Color32::from_rgb(17, 18, 20);
const CARD: Color32 = Color32::from_rgb(26, 27, 30);
const BORDER: Color32 = Color32::from_rgb(38, 39, 43);
const AMBER: Color32 = Color32::from_rgb(245, 166, 13);

/// Home reads recent folders from the index.
#[derive(Default)]
pub struct HomeView;

impl HomeView {
    /// Draws the launch screen and reports the user's next action.
    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db) -> Option<Action> {
        let mut action = None;
        let bounds = ui.max_rect();
        ui.painter().rect_filled(bounds, 0.0, BACKGROUND);
        for row in 0..(bounds.height() / 26.0) as usize {
            for col in 0..(bounds.width() / 26.0) as usize {
                ui.painter().circle_filled(
                    bounds.min + vec2(col as f32 * 26.0, row as f32 * 26.0),
                    0.65,
                    Color32::from_rgb(29, 29, 30),
                );
            }
        }
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(20.0, 24.0), egui::Sense::hover());
            aperture(ui.painter(), rect.center(), 7.0);
            ui.label(
                egui::RichText::new("Culltronic")
                    .strong()
                    .color(theme::TEXT),
            );
            ui.label(
                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(theme::MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("ⓘ  About").frame(false)).clicked() {
                    action = Some(Action::ShowAbout);
                }
            });
        });
        ui.separator();
        let recents = db.recent_roots(10).unwrap_or_else(|error| {
            tracing::warn!(%error, "cannot load recent folders");
            Vec::new()
        });
        let footer =
            egui::Rect::from_min_max(pos2(bounds.left(), bounds.bottom() - 38.0), bounds.max);
        let mut hints = egui::text::LayoutJob::default();
        hints.wrap.max_width = footer.width();
        for (index, (key, label)) in [
            ("Space", "Preview"),
            ("1\u{2013}5", "Color labels"),
            ("F", "Filter"),
            ("?", "Shortcuts"),
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                hints.append("     ", 0.0, egui::TextFormat::default());
            }
            hints.append(
                key,
                0.0,
                egui::TextFormat {
                    font_id: FontId::proportional(10.0),
                    color: theme::TEXT,
                    background: BORDER,
                    ..Default::default()
                },
            );
            hints.append(
                &format!("  {label}"),
                0.0,
                egui::TextFormat {
                    font_id: FontId::proportional(10.0),
                    color: theme::MUTED,
                    ..Default::default()
                },
            );
        }
        let hints = ui.painter().layout_job(hints);
        ui.painter()
            .galley(footer.center() - hints.size() * 0.5, hints, theme::MUTED);
        egui::ScrollArea::vertical()
            .max_height((footer.top() - ui.cursor().top() - 12.0).max(0.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(((bounds.height() - 560.0) * 0.42).max(24.0));
                let width = 360.0_f32.min(ui.available_width());
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), 0.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        let (badge, _) =
                            ui.allocate_exact_size(vec2(52.0, 52.0), egui::Sense::hover());
                        ui.painter().rect(
                            badge,
                            14.0,
                            Color32::from_rgb(43, 34, 17),
                            Stroke::new(1.0, Color32::from_rgb(83, 59, 17)),
                            egui::StrokeKind::Inside,
                        );
                        aperture(ui.painter(), badge.center(), 12.0);
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new("Culltronic")
                                .size(34.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new("Instant culling for RAW folders")
                                .size(14.0)
                                .color(theme::MUTED),
                        );
                        ui.add_space(32.0);
                        let button_width = (width - 56.0).max(180.0).min(width);
                        if action_button(ui, button_width, "Open Folder…", Some("Ctrl+O"), true)
                            .clicked()
                        {
                            action = Some(Action::PickFolder);
                        }
                        if let Some(last) = recents.first() {
                            ui.add_space(8.0);
                            let response = action_button(
                                ui,
                                button_width,
                                &format!("Resume — {}", display_name(last)),
                                None,
                                false,
                            );
                            if response.clicked() {
                                action = Some(Action::OpenFolder(last.clone()));
                            }
                            response.on_hover_text(last.display().to_string());
                        }
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("or drag and drop a folder here")
                                .size(11.0)
                                .color(theme::MUTED),
                        );
                        ui.add_space(36.0);
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(width);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("RECENT FOLDERS")
                                            .size(10.0)
                                            .color(theme::MUTED),
                                    );
                                    if !recents.is_empty() {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new("Clear History")
                                                                .size(10.0)
                                                                .color(theme::MUTED),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .clicked()
                                                {
                                                    action = Some(Action::ClearRecentFolders);
                                                }
                                            },
                                        );
                                    }
                                });
                                ui.separator();
                                if recents.is_empty() {
                                    ui.add_space(8.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "Your recently opened folders will appear here.",
                                        )
                                        .size(12.0)
                                        .color(theme::MUTED),
                                    );
                                }
                                for (index, path) in recents.iter().enumerate() {
                                    let (rect, response) = ui.allocate_exact_size(
                                        vec2(width, 48.0),
                                        egui::Sense::click(),
                                    );
                                    ui.painter().rect(
                                        rect,
                                        5.0,
                                        if response.hovered() {
                                            Color32::from_rgb(34, 34, 37)
                                        } else {
                                            CARD
                                        },
                                        Stroke::new(1.0, BORDER),
                                        egui::StrokeKind::Inside,
                                    );
                                    folder_icon(
                                        ui.painter(),
                                        pos2(rect.left() + 15.0, rect.center().y),
                                        if index == 0 { AMBER } else { theme::MUTED },
                                    );
                                    let text_rect = rect.shrink2(vec2(36.0, 0.0));
                                    let clipped = ui.painter().with_clip_rect(text_rect);
                                    clipped.text(
                                        pos2(text_rect.left(), rect.top() + 8.0),
                                        Align2::LEFT_TOP,
                                        display_name(path),
                                        FontId::proportional(12.0),
                                        theme::TEXT,
                                    );
                                    clipped.text(
                                        pos2(text_rect.left(), rect.top() + 27.0),
                                        Align2::LEFT_TOP,
                                        path.display().to_string(),
                                        FontId::proportional(10.0),
                                        theme::MUTED,
                                    );
                                    response.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            ui.is_enabled(),
                                            display_name(path),
                                        )
                                    });
                                    if response.clicked() {
                                        action = Some(Action::OpenFolder(path.clone()));
                                    }
                                    response.on_hover_text(path.display().to_string());
                                    ui.add_space(3.0);
                                }
                            },
                        );
                        ui.add_space(20.0);
                    },
                );
            });
        if ui
            .ctx()
            .input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::O))
        {
            action = Some(Action::PickFolder);
        }
        action
    }
}

fn action_button(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
    hint: Option<&str>,
    primary: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 42.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), title)
    });
    let ink = if primary { BACKGROUND } else { AMBER };
    let fill = if primary {
        if response.hovered() {
            Color32::from_rgb(255, 184, 40)
        } else {
            AMBER
        }
    } else if response.hovered() {
        Color32::from_rgb(39, 32, 19)
    } else {
        BACKGROUND
    };
    ui.painter().rect(
        rect,
        7.0,
        fill,
        Stroke::new(1.0, AMBER),
        egui::StrokeKind::Inside,
    );
    folder_icon(ui.painter(), pos2(rect.left() + 20.0, rect.center().y), ink);
    let right_padding = if hint.is_some() { 76.0 } else { 12.0 };
    let clip = egui::Rect::from_min_max(
        pos2(rect.left() + 36.0, rect.top()),
        pos2(rect.right() - right_padding, rect.bottom()),
    );
    ui.painter().with_clip_rect(clip).text(
        pos2(clip.left(), rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.0),
        ink,
    );
    if let Some(hint) = hint {
        let badge = egui::Rect::from_center_size(
            pos2(rect.right() - 39.0, rect.center().y),
            vec2(56.0, 23.0),
        );
        ui.painter()
            .rect_filled(badge, 4.0, Color32::from_black_alpha(25));
        ui.painter().text(
            badge.center(),
            Align2::CENTER_CENTER,
            hint,
            FontId::proportional(10.0),
            ink,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn folder_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let points = vec![
        center + vec2(-5.0, -4.0),
        center + vec2(-1.0, -4.0),
        center + vec2(1.0, -2.0),
        center + vec2(6.0, -2.0),
        center + vec2(6.0, 5.0),
        center + vec2(-5.0, 5.0),
        center + vec2(-5.0, -4.0),
    ];
    painter.add(egui::Shape::line(points, Stroke::new(1.0, color)));
}

fn aperture(painter: &egui::Painter, center: egui::Pos2, radius: f32) {
    painter.circle_stroke(center, radius, Stroke::new(1.5, AMBER));
    let points: Vec<_> = (0..6)
        .map(|n| {
            let angle = n as f32 * std::f32::consts::TAU / 6.0;
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    for n in 0..6 {
        let inner = center + (points[(n + 2) % 6] - center) * 0.47;
        painter.line_segment([points[n], inner], Stroke::new(1.3, AMBER));
        painter.line_segment(
            [inner, center + (points[(n + 3) % 6] - center) * 0.47],
            Stroke::new(1.3, AMBER),
        );
    }
}

fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}
