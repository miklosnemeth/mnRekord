//! The look: a dark desk with cards, a stage for the region, and a slim recording bar.
use crate::app::*;
use crate::config::*;
use crate::win;
use eframe::egui::{self, Align, Color32, CornerRadius, FontId, Layout, Pos2, Rect as URect, RichText, Sense, Shadow, Stroke, StrokeKind};
use std::sync::Arc;

const PAPER: Color32 = Color32::from_rgb(0x16, 0x18, 0x1D);
const CARD: Color32 = Color32::from_rgb(0x20, 0x23, 0x2A);
const INK: Color32 = Color32::from_rgb(0xE8, 0xEA, 0xEE);
const OVERLAY: Color32 = Color32::from_rgb(0x1F, 0x24, 0x30);
const WIDGET: Color32 = Color32::from_rgb(0x2A, 0x2E, 0x37);
const WIDGET_HOVER: Color32 = Color32::from_rgb(0x34, 0x39, 0x44);
const WIDGET_ACTIVE: Color32 = Color32::from_rgb(0x3E, 0x43, 0x50);
const OFF: Color32 = Color32::from_rgb(0x47, 0x4C, 0x57);
const MUTED: Color32 = Color32::from_rgb(0x9A, 0xA0, 0xAC);
const LINE: Color32 = Color32::from_rgb(0x2E, 0x32, 0x3B);
const CORAL: Color32 = Color32::from_rgb(0xE5, 0x53, 0x3D);
const TEAL: Color32 = Color32::from_rgb(0x1F, 0x9E, 0x8F);
const AMBER: Color32 = Color32::from_rgb(0xE9, 0xA2, 0x3B);
const STAGE: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);

pub fn apply_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, file: &str, family: egui::FontFamily| {
        if let Ok(b) = std::fs::read(format!(r"C:\Windows\Fonts\{file}")) {
            fonts.font_data.insert(name.to_owned(), Arc::new(egui::FontData::from_owned(b)));
            fonts.families.entry(family).or_default().insert(0, name.to_owned());
        }
    };
    add("segoe", "segoeui.ttf", egui::FontFamily::Proportional);
    add("segoe-bold", "seguisb.ttf", egui::FontFamily::Name("bold".into()));
    fonts.families.entry(egui::FontFamily::Name("bold".into())).or_default().extend(["Ubuntu-Light".to_owned()]);
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = PAPER;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.override_text_color = Some(INK);
    v.selection.bg_fill = CORAL.gamma_multiply(0.25);
    v.selection.stroke = Stroke::new(1.0, CORAL);
    v.hyperlink_color = CORAL;
    let r = CornerRadius::same(5);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open, &mut v.widgets.noninteractive] {
        w.corner_radius = r;
    }
    v.widgets.inactive.weak_bg_fill = WIDGET;
    v.widgets.inactive.bg_fill = WIDGET;
    v.widgets.hovered.weak_bg_fill = WIDGET_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.active.weak_bg_fill = WIDGET_ACTIVE;
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.style_mut_of(egui::Theme::Light, |s| {
        use egui::TextStyle::*;
        s.text_styles.insert(Body, FontId::proportional(14.0));
        s.text_styles.insert(Button, FontId::proportional(14.0));
        s.text_styles.insert(Small, FontId::proportional(12.0));
        s.text_styles.insert(Heading, FontId::new(20.0, egui::FontFamily::Name("bold".into())));
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
    });
}

fn bold(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("bold".into()))
}

fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(5)
        .inner_margin(14)
        .shadow(Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(14) })
        .show(ui, add)
        .inner
}

fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(38.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let bg = OFF.lerp_to_gamma(TEAL, t);
    let p = ui.painter();
    p.rect_filled(rect, 5, bg);
    p.circle_filled(egui::pos2(rect.left() + 11.0 + t * 16.0, rect.center().y), 8.0, Color32::WHITE);
    resp
}

#[derive(Clone, Copy)]
enum Icon {
    Screen,
    Speaker,
    Gear,
    Close,
}

fn paint_icon(p: &egui::Painter, c: Pos2, icon: Icon, col: Color32) {
    let s = Stroke::new(1.6, col);
    let box_at = |x: f32, y: f32, w: f32, h: f32| URect::from_center_size(c + egui::vec2(x, y), egui::vec2(w, h));
    match icon {
        Icon::Screen => {
            p.rect_stroke(box_at(0.0, -2.0, 18.0, 12.0), 2, s, StrokeKind::Inside);
            p.line_segment([c + egui::vec2(-5.0, 7.0), c + egui::vec2(5.0, 7.0)], s);
        }
        Icon::Speaker => {
            p.add(egui::Shape::convex_polygon(
                vec![c + egui::vec2(-8.0, -3.0), c + egui::vec2(-4.0, -3.0), c + egui::vec2(1.0, -7.0), c + egui::vec2(1.0, 7.0), c + egui::vec2(-4.0, 3.0), c + egui::vec2(-8.0, 3.0)],
                col,
                Stroke::NONE,
            ));
            p.line_segment([c + egui::vec2(4.0, -3.0), c + egui::vec2(4.0, 3.0)], s);
            p.line_segment([c + egui::vec2(7.0, -5.0), c + egui::vec2(7.0, 5.0)], s);
        }
        Icon::Gear => {
            p.circle_stroke(c, 5.0, s);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = egui::vec2(a.cos(), a.sin());
                p.line_segment([c + d * 6.0, c + d * 9.0], Stroke::new(2.2, col));
            }
        }
        Icon::Close => {
            p.line_segment([c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0)], s);
            p.line_segment([c + egui::vec2(5.0, -5.0), c + egui::vec2(-5.0, 5.0)], s);
        }
    }
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 5, WIDGET_HOVER);
    }
    paint_icon(ui.painter(), rect.center(), icon, INK);
    resp.on_hover_text(tip)
}

fn meter(ui: &mut egui::Ui, width: f32, level: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), Sense::hover());
    let n = (width / 7.0).floor().max(4.0) as usize;
    let step = width / n as f32;
    // Levels read better on a root curve: quiet speech still moves the bar.
    let lit = (level.clamp(0.0, 1.0).sqrt() * n as f32).ceil() as usize;
    for i in 0..n {
        let x = rect.left() + i as f32 * step;
        let col = if i >= lit {
            WIDGET
        } else if i as f32 > n as f32 * 0.8 {
            AMBER
        } else {
            TEAL
        };
        ui.painter().rect_filled(URect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(step - 2.0, 8.0)), 2, col);
    }
}

fn pill(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(fg)).fill(fill).corner_radius(5).min_size(egui::vec2(0.0, 32.0)))
}

impl App {
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        if let Mode::Picking(_) = self.mode {
            self.draw_picker(ui);
            return;
        }
        if self.rec.is_some() || self.view == "bar" {
            self.draw_bar(ui);
            self.draw_countdown(ui.ctx());
            return;
        }
        self.draw_top(ui);
        self.draw_sources(ui);
        self.draw_stage(ui);
        self.draw_prefs(ui);
        self.draw_prompt(ui);
    }

    fn draw_top(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top").exact_size(58.0).frame(egui::Frame::new().fill(PAPER).inner_margin(egui::Margin::symmetric(18, 12))).show_separator_line(false).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 13.0, CORAL);
                ui.painter().circle_filled(r.center(), 5.0, Color32::WHITE);
                ui.label(RichText::new(APP_NAME).font(bold(19.0)));
                let msg = if self.can_start() { "Ready to record" } else { "Choose something to record" };
                let col = if self.can_start() { TEAL } else { AMBER };
                egui::Frame::new().fill(col.gamma_multiply(0.14)).corner_radius(5).inner_margin(egui::Margin::symmetric(10, 3)).show(ui, |ui| {
                    ui.label(RichText::new(msg).size(12.0).color(col.gamma_multiply(0.9)));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Gear, "Preferences").clicked() {
                        self.prefs_open = true;
                    }
                });
            });
        });
    }

    fn record_button(&mut self, ui: &mut egui::Ui) {
        let can = self.can_start();
        let label = RichText::new(format!("●  Record   {}", self.settings.shortcut_record.label())).font(bold(16.0)).color(Color32::WHITE);
        let btn = egui::Button::new(label).fill(if can { CORAL } else { OFF }).corner_radius(5);
        let size = egui::vec2(ui.available_width(), 46.0);
        if ui.add_enabled(can, |ui: &mut egui::Ui| ui.add_sized(size, btn)).clicked() {
            self.start_recording();
        }
        if let Some(p) = self.last_saved.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Last recording").small().color(MUTED));
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if ui.link(RichText::new(name).small()).clicked() {
                    win::show_in_folder(&p);
                }
            });
        }
        if let Some(n) = &self.notice {
            ui.label(RichText::new(n).small().color(CORAL));
        }
    }

    fn draw_sources(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("sources").exact_size(330.0).resizable(false).frame(egui::Frame::new().fill(PAPER).inner_margin(egui::Margin { left: 18, right: 10, top: 0, bottom: 0 })).show_separator_line(false).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                self.card_screen(ui);
                self.card_system(ui);
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    self.stage_controls(ui);
                });
                self.record_button(ui);
            });
        });
    }

    fn source_head(ui: &mut egui::Ui, icon: Icon, title: &str, on: &mut bool) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(24.0, 22.0), Sense::hover());
            paint_icon(ui.painter(), r.center(), icon, if *on { INK } else { MUTED });
            ui.label(RichText::new(title).font(bold(15.0)).color(if *on { INK } else { MUTED }));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                changed = toggle(ui, on).changed();
            });
        });
        changed
    }

    fn card_screen(&mut self, ui: &mut egui::Ui) {
        let r = self.region();
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            if Self::source_head(ui, Icon::Screen, "Screen", &mut self.settings.record_screen) {
                self.persist();
            }
            let text = if self.settings.record_screen { format!("{} x {} at {} fps", r.w & !1, r.h & !1, self.settings.max_fps) } else { "Off".to_string() };
            ui.label(RichText::new(text).small().color(MUTED));
        });
    }

    fn card_system(&mut self, ui: &mut egui::Ui) {
        let level = self.system.as_ref().map(|s| s.level()).unwrap_or(0.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut on = self.settings.record_system;
            if Self::source_head(ui, Icon::Speaker, "System audio", &mut on) {
                self.set_system_audio(on);
            }
            meter(ui, ui.available_width(), level);
            if let Some(e) = self.system.as_ref().and_then(|s| s.error.lock().unwrap().clone()) {
                ui.label(RichText::new(e).small().color(CORAL));
            }
        });
    }

    // ---- the stage: where the region is set ----

    fn draw_stage(&mut self, ui: &mut egui::Ui) {
        if self.demo && self.demo_tex.is_none() {
            self.demo_tex = Some(ui.ctx().load_texture("demo", demo_picture(), egui::TextureOptions::LINEAR));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PAPER).inner_margin(egui::Margin { left: 8, right: 18, top: 0, bottom: 0 })).show(ui, |ui| {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(ui.available_height().max(0.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Stage").font(bold(15.0)));
                    ui.label(RichText::new("Drag the frame, or its edges, to choose what gets recorded.").small().color(MUTED));
                });
                let size = egui::vec2(ui.available_width(), (ui.available_height() - 4.0).max(120.0));
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
                self.stage_map(ui, rect, &resp);
            });
        });
    }

    fn stage_map(&mut self, ui: &mut egui::Ui, rect: URect, resp: &egui::Response) {
        let vs = self.vscreen;
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 5, STAGE);
        let scale = ((rect.width() - 24.0) / vs.w as f32).min((rect.height() - 24.0) / vs.h as f32);
        let origin = rect.center() - egui::vec2(vs.w as f32, vs.h as f32) * scale / 2.0;
        let to_ui = |x: i32, y: i32| origin + egui::vec2((x - vs.x) as f32, (y - vs.y) as f32) * scale;
        let u_rect = |r: &Rect| URect::from_min_max(to_ui(r.x, r.y), to_ui(r.right(), r.bottom()));
        let tex = self.preview.as_ref().and_then(|t| t.tex.as_ref()).or(self.demo_tex.as_ref()).map(|t| t.id());
        for (i, m) in self.monitors.iter().enumerate() {
            let r = u_rect(m);
            p.rect_filled(r, 5, WIDGET);
            if let Some(id) = tex {
                let uv = URect::from_min_max(
                    egui::pos2((m.x - vs.x) as f32 / vs.w as f32, (m.y - vs.y) as f32 / vs.h as f32),
                    egui::pos2((m.right() - vs.x) as f32 / vs.w as f32, (m.bottom() - vs.y) as f32 / vs.h as f32),
                );
                p.image(id, r, uv, Color32::WHITE);
            }
            p.rect_stroke(r, 5, Stroke::new(1.0, OFF), StrokeKind::Inside);
            p.text(r.left_top() + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, format!("Screen {}", i + 1), FontId::proportional(11.0), Color32::from_white_alpha(230));
        }
        if !self.settings.record_screen {
            return;
        }
        let region = self.region();
        let r = u_rect(&region);
        // Dim what's outside the frame.
        let dim = Color32::from_black_alpha(90);
        let whole = u_rect(&vs);
        p.rect_filled(URect::from_min_max(whole.left_top(), egui::pos2(whole.right(), r.top())), 0, dim);
        p.rect_filled(URect::from_min_max(egui::pos2(whole.left(), r.bottom()), whole.right_bottom()), 0, dim);
        p.rect_filled(URect::from_min_max(egui::pos2(whole.left(), r.top()), egui::pos2(r.left(), r.bottom())), 0, dim);
        p.rect_filled(URect::from_min_max(egui::pos2(r.right(), r.top()), egui::pos2(whole.right(), r.bottom())), 0, dim);
        p.rect_stroke(r, 5, Stroke::new(2.0, CORAL), StrokeKind::Outside);
        let grips = [
            (Grip::NW, r.left_top()),
            (Grip::N, r.center_top()),
            (Grip::NE, r.right_top()),
            (Grip::E, r.right_center()),
            (Grip::SE, r.right_bottom()),
            (Grip::S, r.center_bottom()),
            (Grip::SW, r.left_bottom()),
            (Grip::W, r.left_center()),
        ];
        for (_, g) in grips {
            p.circle(g, 5.0, Color32::WHITE, Stroke::new(2.0, CORAL));
        }
        let hit = |pos: Pos2| -> Option<Grip> {
            grips.iter().find(|(_, g)| g.distance(pos) < 9.0).map(|(k, _)| *k).or(r.contains(pos).then_some(Grip::Move))
        };
        if let Some(pos) = resp.hover_pos() {
            let icon = match hit(pos) {
                Some(Grip::Move) => egui::CursorIcon::Grab,
                Some(Grip::N) | Some(Grip::S) => egui::CursorIcon::ResizeVertical,
                Some(Grip::E) | Some(Grip::W) => egui::CursorIcon::ResizeHorizontal,
                Some(Grip::NE) | Some(Grip::SW) => egui::CursorIcon::ResizeNeSw,
                Some(_) => egui::CursorIcon::ResizeNwSe,
                None => egui::CursorIcon::Default,
            };
            ui.ctx().set_cursor_icon(icon);
        }
        if resp.drag_started() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.grip = hit(pos).map(|g| (g, pos, region));
            }
        }
        if let (true, Some((g, from, start)), Some(pos)) = (resp.dragged(), self.grip, resp.interact_pointer_pos()) {
            let dx = ((pos.x - from.x) / scale) as i32;
            let dy = ((pos.y - from.y) / scale) as i32;
            self.settings.region = Some(match self.settings.aspect {
                Some(a) => drag_region_locked(start, g, dx, dy, &vs, a),
                None => drag_region(start, g, dx, dy, &vs),
            });
        }
        if resp.drag_stopped() {
            self.grip = None;
            self.persist();
        }
    }

    fn stage_controls(&mut self, ui: &mut egui::Ui) {
        let vs = self.vscreen;
        let region = self.region();
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal_wrapped(|ui| {
            for (i, m) in self.monitors.clone().iter().enumerate() {
                let selected = region == *m;
                if ui.selectable_label(selected, format!("Screen {}", i + 1)).clicked() {
                    self.set_region_fitted(*m);
                }
            }
            if ui.add(egui::Button::new("Draw on screen").stroke(Stroke::new(1.0, LINE)).corner_radius(5).min_size(egui::vec2(0.0, 30.0))).clicked() {
                self.begin_pick();
            }
        });
        let (mut w, mut h) = (region.w, region.h);
        ui.horizontal(|ui| {
            ui.add_sized([44.0, 28.0], egui::Label::new(RichText::new("Width").small().color(MUTED)));
            let a = ui.add(egui::DragValue::new(&mut w).range(MIN_REGION..=vs.w));
            ui.add_space(8.0);
            ui.add_sized([44.0, 28.0], egui::Label::new(RichText::new("Height").small().color(MUTED)));
            let b = ui.add(egui::DragValue::new(&mut h).range(MIN_REGION..=vs.h));
            if a.changed() || b.changed() {
                if let Some((rw, rh)) = self.settings.aspect {
                    if a.changed() {
                        h = (w as i64 * rh as i64 / rw as i64) as i32;
                    } else {
                        w = (h as i64 * rw as i64 / rh as i64) as i32;
                    }
                    let k = 1.0f64.min(vs.w as f64 / w as f64).min(vs.h as f64 / h as f64);
                    w = (w as f64 * k) as i32;
                    h = (w as i64 * rh as i64 / rw as i64) as i32;
                }
                self.set_region(Rect::new(region.x, region.y, w, h));
            }
        });
        // The ratio is the base; the sizes follow it: landscape for 16:9, portrait for 9:16.
        let sizes: Vec<(&str, i32, i32)> = match self.settings.aspect {
            Some((16, 9)) => SIZE_TIERS.iter().map(|&(n, w)| (n, w, w * 9 / 16)).collect(),
            Some((9, 16)) => SIZE_TIERS.iter().map(|&(n, w)| (n, w * 9 / 16, w)).collect(),
            _ => PRESETS.iter().filter(|p| p.0 == "Wide").map(|&(_, n, w, h)| (n, w, h)).collect(),
        };
        ui.horizontal(|ui| {
            ui.add_sized([44.0, 28.0], egui::Label::new(RichText::new("Ratio").small().color(MUTED)));
            let name = |a: Option<(i32, i32)>| match a {
                Some((w, h)) => format!("{w}:{h}"),
                None => "Free".to_string(),
            };
            egui::ComboBox::from_id_salt("ratio").width(190.0).selected_text(name(self.settings.aspect)).show_ui(ui, |ui| {
                for a in [Some((16, 9)), Some((9, 16)), None] {
                    if ui.selectable_label(self.settings.aspect == a, name(a)).clicked() {
                        self.set_aspect(a);
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add_sized([44.0, 28.0], egui::Label::new(RichText::new("Size").small().color(MUTED)));
            let current = sizes
                .iter()
                .find(|s| s.1 == region.w && s.2 == region.h)
                .map(|s| format!("{} ({} x {})", s.0, s.1, s.2))
                .unwrap_or_else(|| format!("Custom ({} x {})", region.w, region.h));
            egui::ComboBox::from_id_salt("size").width(190.0).selected_text(current).show_ui(ui, |ui| {
                for (name, w, h) in sizes {
                    let fits = w <= vs.w && h <= vs.h;
                    let selected = region.w == w && region.h == h;
                    let item = ui.add_enabled(fits, egui::Button::selectable(selected, format!("{name} ({w} x {h})")));
                    if item.clicked() {
                        // Keep the frame centred where it is, as far as the new size allows.
                        let cx = region.x + region.w / 2;
                        let cy = region.y + region.h / 2;
                        self.set_region(Rect::new(cx - w / 2, cy - h / 2, w, h));
                        ui.close();
                    }
                }
            });
        });
    }

    // ---- preferences ----

    fn draw_prefs(&mut self, ui: &mut egui::Ui) {
        if !self.prefs_open {
            return;
        }
        let ctx = ui.ctx().clone();
        let modal = egui::Modal::new(egui::Id::unique("prefs")).frame(
            egui::Frame::new().fill(CARD).corner_radius(5).inner_margin(20).stroke(Stroke::new(1.0, LINE)),
        );
        let resp = modal.show(&ctx, |ui| {
            ui.set_width(560.0);
            ui.set_min_height(380.0);
            ui.horizontal(|ui| {
                ui.heading("Preferences");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, "Close").clicked() {
                        self.close_prefs();
                    }
                });
            });
            ui.horizontal(|ui| {
                for (i, t) in ["General", "Inputs", "Files", "Shortcuts"].iter().enumerate() {
                    if ui.selectable_label(self.prefs_tab == i, *t).clicked() {
                        self.prefs_tab = i;
                    }
                }
            });
            ui.separator();
            match self.prefs_tab {
                0 => self.prefs_general(ui),
                1 => self.prefs_inputs(ui),
                2 => self.prefs_files(ui),
                _ => self.prefs_shortcuts(ui),
            }
        });
        if resp.should_close() && self.capture_key.is_none() {
            self.close_prefs();
        }
    }

    fn close_prefs(&mut self) {
        self.prefs_open = false;
        self.capture_key = None;
        self.register_shortcuts();
        self.persist();
    }

    fn prefs_general(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.settings;
        let rows: [(&str, &mut bool); 7] = [
            ("Show recorder countdown", &mut s.show_countdown),
            ("Do not capture recorder on screen", &mut s.use_window_mask),
            ("Restore cursor location after pause", &mut s.restore_cursor),
            ("Exit after stopped", &mut s.exit_on_stop),
            ("Open recording after stopped", &mut s.open_on_stop),
            ("Show region indicator", &mut s.show_region_indicator),
            ("Automatically minimize recording window", &mut s.auto_minimize),
        ];
        for (label, v) in rows {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    toggle(ui, v);
                });
            });
        }
    }

    fn prefs_inputs(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Screen").font(bold(15.0)));
        ui.horizontal(|ui| {
            ui.label("Maximum frame rate");
            for f in SCREEN_RATES {
                ui.selectable_value(&mut self.settings.max_fps, f, format!("{f} fps"));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Encoder");
            ui.selectable_value(&mut self.settings.encoder, Encoder::Standard, "Standard");
            ui.selectable_value(&mut self.settings.encoder, Encoder::High, "High detail");
        });
    }

    fn prefs_files(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Name").font(bold(15.0)));
        ui.radio_value(&mut self.settings.prompt_for_name, false, "Automatic file name");
        ui.radio_value(&mut self.settings.prompt_for_name, true, "Prompt for file name");
        ui.separator();
        folder_row(ui, "Default output folder", &mut self.settings.output_folder);
        folder_row(ui, "Temporary storage folder", &mut self.settings.temp_folder);
    }

    fn prefs_shortcuts(&mut self, ui: &mut egui::Ui) {
        let rows = ["Record, pause and resume", "Stop", "Add marker"];
        for (i, name) in rows.iter().enumerate() {
            let current = self.shortcut(i).label();
            ui.horizontal(|ui| {
                ui.label(*name);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let text = if self.capture_key == Some(i) { "Press a key...".to_string() } else { current };
                    if ui.button(text).clicked() {
                        self.capture_key = Some(i);
                        self.shortcut_msg = None;
                        self.release_shortcuts();
                    }
                });
            });
        }
        if let Some(i) = self.capture_key {
            self.read_shortcut_key(ui.ctx(), i);
        }
        if ui.button("Restore defaults").clicked() {
            let d = Settings::default();
            self.settings.shortcut_record = d.shortcut_record;
            self.settings.shortcut_stop = d.shortcut_stop;
            self.settings.shortcut_marker = d.shortcut_marker;
            self.capture_key = None;
            self.shortcut_msg = None;
            self.register_shortcuts();
        }
        if let Some(m) = &self.shortcut_msg {
            ui.label(RichText::new(m).color(CORAL));
        }
        let failed = self.hotkeys.as_ref().map(|h| h.failed.lock().unwrap().clone()).unwrap_or_default();
        if !failed.is_empty() {
            ui.label(RichText::new(format!("Could not assign the following shortcuts: {}", failed.join(", "))).color(CORAL));
        }
    }

    fn shortcut(&mut self, i: usize) -> &mut Shortcut {
        match i {
            0 => &mut self.settings.shortcut_record,
            1 => &mut self.settings.shortcut_stop,
            _ => &mut self.settings.shortcut_marker,
        }
    }

    fn read_shortcut_key(&mut self, ctx: &egui::Context, i: usize) {
        let pressed = ctx.input(|inp| {
            inp.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        let Some((key, m)) = pressed else { return };
        self.capture_key = None;
        if key != egui::Key::Escape {
            let sc = Shortcut { key: key.name().to_string(), ctrl: m.ctrl, alt: m.alt, shift: m.shift };
            let others = [0, 1, 2].into_iter().filter(|o| *o != i).find(|o| *self.shortcut(*o) == sc);
            if sc.vk().is_none() {
                self.shortcut_msg = Some(format!("{} can't be used as a shortcut", sc.label()));
            } else if let Some(o) = others {
                let name = ["Record, pause and resume", "Stop", "Add marker"][o];
                self.shortcut_msg = Some(format!("The shortcut {} conflicts with {}", sc.label(), name));
            } else {
                *self.shortcut(i) = sc;
                self.shortcut_msg = None;
            }
        }
        self.register_shortcuts();
    }

    // ---- prompts in the main window ----

    fn draw_prompt(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (title, body, a, b) = match &self.prompt {
            Some(Prompt::Recover(_)) => ("Recover recording", "A recording wasn't finished the last time. Keep what was recorded?", "Recover", "Delete"),
            Some(Prompt::SaveOrDelete(_)) => ("Save recording", "The recording hasn't been saved yet.", "Save", "Delete"),
            _ => return,
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("prompt")).frame(egui::Frame::new().fill(CARD).corner_radius(5).inner_margin(20)).show(&ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(title);
            ui.label(body);
            ui.horizontal(|ui| {
                if pill(ui, a, CORAL, Color32::WHITE).clicked() {
                    answer = Some(true);
                }
                if pill(ui, b, CARD, INK).clicked() {
                    answer = Some(false);
                }
            });
        });
        let Some(save) = answer else { return };
        let dir = match self.prompt.take() {
            Some(Prompt::Recover(d)) | Some(Prompt::SaveOrDelete(d)) => d,
            _ => return,
        };
        if !save {
            self.delete_folder(&dir);
            self.start_preview();
            return;
        }
        let picked = rfd::FileDialog::new().add_filter("MP4 video", &["mp4"]).set_file_name("recovered.mp4").set_directory(&self.settings.output_folder).save_file();
        match picked {
            Some(p) => {
                self.save_to(&dir, p.with_extension("mp4"));
                if !self.settings.recovery_location.is_empty() {
                    // Saving failed: offer it again.
                    self.prompt = Some(Prompt::Recover(dir));
                }
            }
            None => self.prompt = Some(Prompt::Recover(dir)),
        }
        self.start_preview();
    }

    // ---- recording bar ----

    fn draw_bar(&mut self, ui: &mut egui::Ui) {
        let (timer, running, counting) = match &self.rec {
            Some(r) => (r.timer_text.clone(), r.session.shared.clock.running(), r.count_from.is_some()),
            None => ("00:01:24".to_string(), true, false),
        };
        let frame = egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, LINE)).inner_margin(egui::Margin::symmetric(14, 10));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            let whole = ui.max_rect();
            let drag = ui.interact(whole, egui::Id::unique("bar-drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if let Some(Prompt::Delete { quit, was_running }) = self.prompt {
                let msg = if quit { "Are you sure you want to delete this recording and quit?" } else { "Are you sure you want to delete this recording?" };
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new(msg).font(bold(14.0)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if pill(ui, "No", CARD, INK).clicked() {
                            self.answer_delete(false, quit, was_running);
                        }
                        if pill(ui, "Yes", CORAL, Color32::WHITE).clicked() {
                            self.answer_delete(true, quit, was_running);
                        }
                    });
                });
                return;
            }
            ui.horizontal_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                let t = ui.input(|i| i.time);
                let lit = !running || (t * 2.0) as i64 % 2 == 0;
                ui.painter().circle_filled(r.center(), 6.0, if lit { if running { CORAL } else { AMBER } } else { CORAL.gamma_multiply(0.3) });
                ui.label(RichText::new(timer).font(bold(20.0)));
                ui.add_space(6.0);
                let pause_text = if running || counting { "Pause" } else { "Resume" };
                if ui.add_enabled(!counting, egui::Button::new(pause_text).corner_radius(5).min_size(egui::vec2(72.0, 32.0))).clicked() {
                    self.toggle_pause();
                }
                if pill(ui, "Stop", CORAL, Color32::WHITE).on_hover_text(self.settings.shortcut_stop.label()).clicked() {
                    self.stop_recording(false);
                    return;
                }
                if ui.add_enabled(self.settings.record_screen, egui::Button::new("Marker").corner_radius(5).min_size(egui::vec2(0.0, 32.0))).on_hover_text(self.settings.shortcut_marker.label()).clicked() {
                    if let Some(r) = &self.rec {
                        r.session.shared.add_marker();
                    }
                }
                if ui.add(egui::Button::new("Restart").corner_radius(5).min_size(egui::vec2(0.0, 32.0))).clicked() {
                    self.ask_delete(false);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, "Delete this recording and quit").clicked() {
                        self.ask_delete(true);
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.add_space(6.0);
                        let s = self.system.as_ref().map(|s| s.level()).unwrap_or(0.0);
                        meter(ui, 70.0, s);
                    });
                });
            });
        });
    }

    fn draw_countdown(&mut self, ctx: &egui::Context) {
        let Some(from) = self.rec.as_ref().and_then(|r| r.count_from) else { return };
        let t = from.elapsed().as_secs_f32() - COUNT_DELAY;
        if t < 0.0 {
            return;
        }
        let number = (COUNT_START - (t / COUNT_STEP) as i32).max(1);
        let ppp = ctx.pixels_per_point();
        let area = if self.settings.record_screen { self.region() } else { self.monitors.first().copied().unwrap_or(self.vscreen) };
        let size = egui::vec2(360.0, 240.0);
        let pos = egui::pos2((area.x + area.w / 2) as f32 / ppp - size.x / 2.0, (area.y + area.h / 2) as f32 / ppp - size.y / 2.0);
        let hint = format!("Press {} to stop the recording.", self.settings.shortcut_stop.label());
        let builder = egui::ViewportBuilder::default()
            .with_title("Countdown")
            .with_inner_size(size)
            .with_position(pos)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_taskbar(false)
            .with_resizable(false);
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("countdown"), builder, move |ui, _| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                let r = ui.max_rect();
                let p = ui.painter();
                p.rect_filled(r, 5, Color32::from_rgba_unmultiplied(0x1F, 0x24, 0x30, 225));
                p.text(r.center() - egui::vec2(0.0, 22.0), egui::Align2::CENTER_CENTER, number.to_string(), bold(110.0), Color32::WHITE);
                p.text(egui::pos2(r.center().x, r.bottom() - 28.0), egui::Align2::CENTER_CENTER, &hint, FontId::proportional(14.0), Color32::from_white_alpha(220));
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        });
    }

    // ---- choosing the region on the desktop itself ----

    fn draw_picker(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let vs = self.vscreen;
        let ppp = ctx.pixels_per_point();
        let mut finish: Option<Option<Rect>> = None;
        let monitors = self.monitors.clone();
        let aspect = self.settings.aspect;
        if let Mode::Picking(p) = &mut self.mode {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                let full = ui.max_rect();
                let resp = ui.allocate_rect(full, Sense::click_and_drag());
                let painter = ui.painter();
                let uv_all = URect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0));
                painter.image(p.tex.id(), full, uv_all, Color32::WHITE);
                let to_px = |pos: Pos2| (vs.x + (pos.x * ppp) as i32, vs.y + (pos.y * ppp) as i32);
                let to_ui = |r: &Rect| URect::from_min_size(egui::pos2((r.x - vs.x) as f32 / ppp, (r.y - vs.y) as f32 / ppp), egui::vec2(r.w as f32 / ppp, r.h as f32 / ppp));
                let hover = ctx.pointer_hover_pos().or(ctx.pointer_interact_pos()).unwrap_or(full.center());
                let (hx, hy) = to_px(hover);
                if resp.drag_started() {
                    p.press = ctx.input(|i| i.pointer.press_origin());
                }
                let chosen = if let Some(from) = p.press {
                    let (ax, ay) = to_px(from);
                    match aspect {
                        Some(a) => locked_box((ax, ay), (hx >= ax, hy >= ay), ((hx - ax).abs() as f64, (hy - ay).abs() as f64), &vs, a),
                        None => Rect::new(ax.min(hx), ay.min(hy), (ax - hx).abs(), (ay - hy).abs()).intersect(&vs).unwrap_or(Rect::new(ax, ay, 0, 0)),
                    }
                } else {
                    let whole = p.windows.iter().find(|w| w.contains(hx, hy)).copied().or(monitors.iter().find(|m| m.contains(hx, hy)).copied()).unwrap_or(vs).intersect(&vs).unwrap_or(vs);
                    match aspect {
                        Some((rw, rh)) => whole.fit_ratio(rw, rh),
                        None => whole,
                    }
                };
                // Everything but the choice is dimmed.
                let c = to_ui(&chosen);
                let dim = Color32::from_black_alpha(110);
                painter.rect_filled(URect::from_min_max(full.left_top(), egui::pos2(full.right(), c.top())), 0, dim);
                painter.rect_filled(URect::from_min_max(egui::pos2(full.left(), c.bottom()), full.right_bottom()), 0, dim);
                painter.rect_filled(URect::from_min_max(egui::pos2(full.left(), c.top()), egui::pos2(c.left(), c.bottom())), 0, dim);
                painter.rect_filled(URect::from_min_max(egui::pos2(c.right(), c.top()), egui::pos2(full.right(), c.bottom())), 0, dim);
                painter.rect_stroke(c, 5, Stroke::new(2.0, CORAL), StrokeKind::Outside);
                let label = format!("{} x {}", chosen.w, chosen.h);
                let lpos = egui::pos2(c.left().max(full.left() + 6.0), (c.top() - 30.0).max(full.top() + 6.0));
                let lr = URect::from_min_size(lpos, egui::vec2(110.0, 24.0));
                painter.rect_filled(lr, 5, OVERLAY);
                painter.text(lr.center(), egui::Align2::CENTER_CENTER, label, FontId::proportional(13.0), Color32::WHITE);
                // Magnifier beside the pointer.
                let m = crate::config::MAGNIFIER_SIZE;
                let span = m / MAGNIFIER_ZOOM;
                let (u, v) = ((hover.x - full.left()) / full.width(), (hover.y - full.top()) / full.height());
                let (du, dv) = (span / full.width() / 2.0, span / full.height() / 2.0);
                let mut mr = URect::from_min_size(hover + egui::vec2(24.0, 24.0), egui::vec2(m, m));
                if mr.right() > full.right() {
                    mr = mr.translate(egui::vec2(-m - 48.0, 0.0));
                }
                if mr.bottom() > full.bottom() {
                    mr = mr.translate(egui::vec2(0.0, -m - 48.0));
                }
                painter.image(p.tex.id(), mr, URect::from_min_max(egui::pos2(u - du, v - dv), egui::pos2(u + du, v + dv)), Color32::WHITE);
                painter.rect_stroke(mr, 5, Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
                painter.line_segment([mr.center_top(), mr.center_bottom()], Stroke::new(1.0, CORAL));
                painter.line_segment([mr.left_center(), mr.right_center()], Stroke::new(1.0, CORAL));
                let help = URect::from_center_size(egui::pos2(full.center().x, full.top() + 40.0), egui::vec2(420.0, 34.0));
                painter.rect_filled(help, 5, OVERLAY);
                painter.text(help.center(), egui::Align2::CENTER_CENTER, "Click a window or screen, or drag a rectangle. Esc cancels.", FontId::proportional(13.0), Color32::WHITE);
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                if resp.drag_stopped() {
                    p.press = None;
                    finish = Some((chosen.w >= MIN_REGION && chosen.h >= MIN_REGION).then_some(chosen));
                } else if resp.clicked() {
                    finish = Some(Some(chosen));
                } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    finish = Some(None);
                }
            });
        }
        if let Some(f) = finish {
            self.end_pick(f);
        }
    }
}

fn drag_region(s: Rect, g: Grip, dx: i32, dy: i32, vs: &Rect) -> Rect {
    if g == Grip::Move {
        return Rect::new(s.x + dx, s.y + dy, s.w, s.h).clamped(vs);
    }
    let (mut l, mut t, mut r, mut b) = (s.x, s.y, s.right(), s.bottom());
    if matches!(g, Grip::W | Grip::NW | Grip::SW) {
        l = (l + dx).clamp(vs.x, r - MIN_REGION);
    }
    if matches!(g, Grip::E | Grip::NE | Grip::SE) {
        r = (r + dx).clamp(l + MIN_REGION, vs.right());
    }
    if matches!(g, Grip::N | Grip::NW | Grip::NE) {
        t = (t + dy).clamp(vs.y, b - MIN_REGION);
    }
    if matches!(g, Grip::S | Grip::SW | Grip::SE) {
        b = (b + dy).clamp(t + MIN_REGION, vs.bottom());
    }
    Rect::new(l, t, r - l, b - t)
}

/// A frame of ratio `a` grown from `anchor` toward the side given by `east`/`south`, as large as
/// `raw` (the pointer distance from the anchor) asks for, kept inside `vs` and above the minimum side.
fn locked_box(anchor: (i32, i32), (east, south): (bool, bool), raw: (f64, f64), vs: &Rect, a: (i32, i32)) -> Rect {
    let ratio = a.0 as f64 / a.1 as f64;
    let avail_w = if east { vs.right() - anchor.0 } else { anchor.0 - vs.x } as f64;
    let avail_h = if south { vs.bottom() - anchor.1 } else { anchor.1 - vs.y } as f64;
    let min_w = (MIN_REGION as f64).max(MIN_REGION as f64 * ratio);
    let max_w = avail_w.min(avail_h * ratio).max(min_w);
    let w = raw.0.max(raw.1 * ratio).clamp(min_w, max_w);
    let h = w / ratio;
    let (w, h) = (w.round() as i32, h.round() as i32);
    Rect::new(if east { anchor.0 } else { anchor.0 - w }, if south { anchor.1 } else { anchor.1 - h }, w, h)
}

/// `drag_region` with a locked ratio: corners grow from the opposite corner, edges around the middle.
fn drag_region_locked(s: Rect, g: Grip, dx: i32, dy: i32, vs: &Rect, a: (i32, i32)) -> Rect {
    if g == Grip::Move {
        return Rect::new(s.x + dx, s.y + dy, s.w, s.h).clamped(vs);
    }
    let ratio = a.0 as f64 / a.1 as f64;
    let east = matches!(g, Grip::E | Grip::NE | Grip::SE);
    let west = matches!(g, Grip::W | Grip::NW | Grip::SW);
    let south = matches!(g, Grip::S | Grip::SE | Grip::SW);
    let north = matches!(g, Grip::N | Grip::NE | Grip::NW);
    let raw_w = s.w as f64 + if east { dx as f64 } else if west { -(dx as f64) } else { 0.0 };
    let raw_h = s.h as f64 + if south { dy as f64 } else if north { -(dy as f64) } else { 0.0 };
    if (east || west) && (north || south) {
        let anchor = (if east { s.x } else { s.right() }, if south { s.y } else { s.bottom() });
        // Both pointer axes count, so a corner follows the mouse in any direction
        // (taking only the larger side would leave the frame stuck on the other axis).
        let size = (raw_w + raw_h * ratio) / 2.0;
        return locked_box(anchor, (east, south), (size, size / ratio), vs, a);
    }
    // Edges: both opposite edges move, so the frame grows around its middle (twice the pointer
    // travel); the middle is pushed back inside the screen when the frame would leave it.
    let (cx, cy) = (s.x + s.w / 2, s.y + s.h / 2);
    let min_w = (MIN_REGION as f64).max(MIN_REGION as f64 * ratio);
    let w = if east || west {
        let raw = s.w as f64 + 2.0 * if east { dx as f64 } else { -(dx as f64) };
        raw.clamp(min_w, (vs.w as f64).min(vs.h as f64 * ratio).max(min_w))
    } else {
        let raw = s.h as f64 + 2.0 * if south { dy as f64 } else { -(dy as f64) };
        let min_h = min_w / ratio;
        raw.clamp(min_h, (vs.h as f64).min(vs.w as f64 / ratio).max(min_h)) * ratio
    };
    let (w, h) = (w.round() as i32, (w / ratio).round() as i32);
    let x = (cx - w / 2).clamp(vs.x, (vs.right() - w).max(vs.x));
    let y = (cy - h / 2).clamp(vs.y, (vs.bottom() - h).max(vs.y));
    Rect::new(x, y, w, h)
}

fn folder_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(ui.available_width() - 90.0));
        if ui.button("Browse").clicked() {
            if let Some(p) = rfd::FileDialog::new().set_directory(value.as_str()).pick_folder() {
                *value = p.to_string_lossy().into_owned();
            }
        }
    });
}

/// A calm stand-in picture for screenshots, so they never show the real desktop.
fn demo_picture() -> egui::ColorImage {
    let (w, h) = (320usize, 180usize);
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (x as f32 / w as f32, y as f32 / h as f32);
            px.extend_from_slice(&[(70.0 + 90.0 * a) as u8, (110.0 + 60.0 * b) as u8, (150.0 + 70.0 * (1.0 - a)) as u8, 255]);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &px)
}
