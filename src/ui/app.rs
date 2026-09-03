use crate::config::Config;
use crate::proxy::ProxyHandle;
use crate::store::model::CaptureRecord;
use crate::store::{export::JsonlWriter, Store};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::theme::{self, space};

enum ProxyState {
    Stopped,
    Starting,
    Running(Arc<Mutex<Option<ProxyHandle>>>),
}

pub struct GrabberApp {
    cfg: Config,
    store: Store,
    rt: tokio::runtime::Handle,
    state: ProxyState,
    captures: Vec<CaptureRecord>,
    selected: Option<uuid::Uuid>,
    status_msg: String,
    export_path: PathBuf,
    last_refresh: std::time::Instant,
}

impl GrabberApp {
    pub fn new(cfg: Config, store: Store, rt: tokio::runtime::Handle) -> Self {
        let export_path = cfg.data_dir().join("export.jsonl");
        Self {
            cfg,
            store,
            rt,
            state: ProxyState::Stopped,
            captures: Vec::new(),
            selected: None,
            status_msg: "Proxy stopped".into(),
            export_path,
            last_refresh: std::time::Instant::now(),
        }
    }

    fn start_proxy(&mut self) {
        let upstream = self.cfg.upstream_base_url.clone();
        let listen = self.cfg.listen_addr.clone();
        let mode = self.cfg.mode;
        let store = self.store.clone();
        self.status_msg = "Starting…".into();

        let handle_slot: Arc<Mutex<Option<ProxyHandle>>> = Arc::new(Mutex::new(None));
        self.state = ProxyState::Starting;

        let slot = handle_slot.clone();
        self.rt.spawn(async move {
            match crate::proxy::server::spawn(&listen, mode, upstream, store).await {
                Ok(h) => {
                    tracing::info!(addr = %h.addr, "proxy started");
                    *slot.lock().await = Some(h);
                }
                Err(e) => tracing::error!(error = %e, "proxy failed to start"),
            }
        });
        self.state = ProxyState::Running(handle_slot);
        self.status_msg = format!("Running on {} ({})", self.cfg.listen_addr, mode.label());
    }

    fn stop_proxy(&mut self) {
        if let ProxyState::Running(slot) = &self.state {
            let slot = slot.clone();
            self.rt.spawn(async move {
                if let Some(h) = slot.lock().await.take() {
                    h.stop();
                }
            });
        }
        self.state = ProxyState::Stopped;
        self.status_msg = "Proxy stopped".into();
    }

    fn refresh_captures(&mut self) {
        let store = self.store.clone();
        match self.rt.block_on(store.list_latest(200)) {
            Ok(list) => self.captures = list,
            Err(e) => tracing::error!(error = %e, "capture refresh failed"),
        }
        self.last_refresh = std::time::Instant::now();
    }

    fn export_jsonl(&mut self) {
        match JsonlWriter::open(self.export_path.clone()) {
            Ok(w) => {
                let mut ok = 0;
                for rec in &self.captures {
                    if w.append(rec).is_ok() {
                        ok += 1;
                    }
                }
                self.status_msg = format!("Exported {ok} records to {}", w.path().display());
            }
            Err(e) => self.status_msg = format!("Export failed: {e}"),
        }
    }
}

impl eframe::App for GrabberApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let p = theme::Palette::DARK;
        let running = matches!(self.state, ProxyState::Running(_));

        // Live refresh: poll the store once a second while the endpoint runs.
        if running {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            if self.last_refresh.elapsed() >= std::time::Duration::from_secs(1) {
                self.refresh_captures();
            }
        }

        // ── Header ────────────────────────────────────────────────────────
        // One primary action per view (Rule 4.2): Start/Stop is the filled
        // button; Export/Refresh are secondary text buttons.
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::default()
                    .fill(p.surface_high)
                    .inner_margin(egui::Margin::symmetric(space::I_L, space::I_M)),
            )
            .show(ctx, |ui| {
                ui.columns(2, |cols| {
                    cols[0].vertical(|ui| {
                        ui.add(egui::Label::new(
                            egui::RichText::new("JanitorAI Grabber").heading().strong(),
                        ));
                        // Status row: color + shape + text (Rule 3.2, never
                        // color alone), muted caption (hierarchy Rule 4.1).
                        ui.horizontal(|ui| {
                            let dot_color = if running {
                                p.success
                            } else {
                                p.on_surface_muted
                            };
                            // Painted circle instead of the "●" character,
                            // which the default egui font cannot render.
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 5.0, dot_color);
                            ui.label(
                                egui::RichText::new(&self.status_msg)
                                    .text_style(egui::TextStyle::Name("Caption".into()))
                                    .color(p.on_surface_muted),
                            );
                        });
                    });
                    cols[1].with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(if running { "Stop" } else { "Start" })
                                    .fill(if running { p.outline } else { p.accent })
                                    .min_size(egui::vec2(96.0, 40.0)),
                            )
                            .clicked()
                        {
                            if running {
                                self.stop_proxy();
                            } else {
                                self.start_proxy();
                            }
                        }
                        if ui
                            .add(egui::Button::new("Export JSONL").fill(egui::Color32::TRANSPARENT))
                            .clicked()
                        {
                            self.export_jsonl();
                        }
                        if ui
                            .add(egui::Button::new("Refresh").fill(egui::Color32::TRANSPARENT))
                            .clicked()
                        {
                            self.refresh_captures();
                        }
                    });
                });
            });

        // ── Capture list ──────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(p.surface)
                    .inner_margin(egui::Margin::same(space::I_L)),
            )
            .show(ctx, |ui| {
                ui.add(egui::Label::new(
                    egui::RichText::new("Captured LLM traffic")
                        .text_style(egui::TextStyle::Name("Title".into())),
                ));
                ui.add_space(space::S);

                if self.captures.is_empty() {
                    // Empty state with the full JanitorAI setup steps inline,
                    // so the user does not need to open the README.
                    ui.add_space(space::L);
                    egui::Frame::default()
                        .fill(p.surface_high)
                        .corner_radius(egui::CornerRadius::same(space::I_RADIUS))
                        .inner_margin(egui::Margin::same(space::I_L))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(
                                egui::RichText::new("How to connect JanitorAI")
                                    .text_style(egui::TextStyle::Name("Title".into()))
                                    .strong(),
                            );
                            ui.add_space(space::XS);
                            ui.label(
                                egui::RichText::new(
                                    "1. Press Start above. The endpoint listens on 127.0.0.1:8817.",
                                )
                                .color(p.on_surface),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "2. Open JanitorAI.com, open your chat, then the API settings (the slider icon in the top bar).",
                                )
                                .color(p.on_surface),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "3. As API choose \"Custom Proxy (OpenAI-compatible)\".",
                                )
                                .color(p.on_surface),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "4. Proxy URL: http://127.0.0.1:8817/v1",
                                )
                                .color(p.on_surface),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "   Proxy password: anything, for example grabber.",
                                )
                                .color(p.on_surface_muted),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "5. Save, then send any message in the chat. It appears here.",
                                )
                                .color(p.on_surface),
                            );
                            ui.add_space(space::S);
                            ui.label(
                                egui::RichText::new(
                                    "Any API key works: the app stores the request and replies with a \
                                     stub success, contacting nothing outside your machine.",
                                )
                                .text_style(egui::TextStyle::Name("Caption".into()))
                                .color(p.on_surface_muted),
                            );
                        });
                    ui.add_space(space::M);
                    ui.label(
                        egui::RichText::new("No captures yet")
                            .text_style(egui::TextStyle::Name("Title".into()))
                            .color(p.on_surface_muted),
                    );
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Cards (common region, Rule 4.3) instead of a dense
                        // grid: each capture is one tappable row card.
                        for rec in &self.captures {
                            let selected = self.selected == Some(rec.id);
                            let row = egui::Frame::default()
                                .fill(if selected { p.outline } else { p.surface_high })
                                .corner_radius(egui::CornerRadius::same(space::I_RADIUS))
                                .inner_margin(egui::Margin::symmetric(space::I_M, space::I_S))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        let dir = match rec.direction {
                                            crate::store::Direction::Request => "→",
                                            crate::store::Direction::Response => "←",
                                        };
                                        ui.label(
                                            egui::RichText::new(
                                                rec.timestamp.format("%H:%M:%S").to_string(),
                                            )
                                            .monospace()
                                            .color(p.on_surface_muted),
                                        );
                                        ui.label(egui::RichText::new(dir).monospace());
                                        ui.label(
                                            egui::RichText::new(&rec.path).monospace().strong(),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if let Some(s) = rec.status {
                                                    let (color, label) = if (200..300).contains(&s)
                                                    {
                                                        (p.success, format!("{s} ok"))
                                                    } else {
                                                        (p.error, format!("{s}"))
                                                    };
                                                    ui.label(
                                                        egui::RichText::new(label).color(color),
                                                    );
                                                }
                                                if rec.secret == crate::store::SecretFlag::Secret {
                                                    ui.label(
                                                        egui::RichText::new("secrets")
                                                            .color(p.warning),
                                                    );
                                                }
                                            },
                                        );
                                    });
                                });
                            let inner = row.response.interact(egui::Sense::click());
                            if inner.clicked() {
                                self.selected = Some(rec.id);
                            }
                            ui.add_space(space::XS);
                        }
                    });
                }
            });

        // ── Detail pop-up ─────────────────────────────────────────────────
        // Modal: not resizable, fills most of the window, closes on the
        // Close button, on Escape, or on a click outside (backdrop).
        if let Some(id) = self.selected {
            if let Some(rec) = self.captures.iter().find(|c| c.id == id) {
                let screen = ctx.content_rect();
                let modal_size = egui::vec2(screen.width() * 0.9, screen.height() * 0.9);
                let area = egui::Area::new(egui::Id::new("capture-detail"))
                    .kind(egui::UiKind::Modal)
                    .sense(egui::Sense::hover())
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .order(egui::Order::Foreground)
                    .movable(false)
                    .default_size(modal_size)
                    .constrain(true)
                    .interactable(true);

                let modal = egui::Modal::new(egui::Id::new("capture-detail-modal"))
                    .area(area)
                    .backdrop_color(egui::Color32::from_black_alpha(120));

                let response = modal.show(ctx, |ui| {
                    // Re-assert the size every frame: Area remembers the size
                    // from the previous frame, so without this the pop-up
                    // keeps the old (bigger) height when the window shrinks.
                    ui.set_max_width(modal_size.x);
                    ui.set_min_width(modal_size.x);
                    ui.set_max_height(modal_size.y);
                    ui.set_min_height(modal_size.y);
                    ui.vertical(|ui| {
                        // Header row: title left, Close right.
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{} {}", rec.method, rec.path))
                                            .text_style(egui::TextStyle::Name("Title".into()))
                                            .strong(),
                                    );
                                    ui.label(match rec.direction {
                                        crate::store::Direction::Request => "request",
                                        crate::store::Direction::Response => "response",
                                    });
                                    if rec.secret == crate::store::SecretFlag::Secret {
                                        ui.label(
                                            egui::RichText::new("contains secrets")
                                                .color(p.warning),
                                        );
                                    }
                                },
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Close").clicked() {
                                        self.selected = None;
                                    }
                                },
                            );
                        });
                        ui.add_space(space::S);
                        ui.separator();

                        ui.strong("Headers (secrets redacted)");
                        egui::ScrollArea::vertical()
                            .id_salt("capture-headers")
                            .auto_shrink([false, false])
                            .max_height(modal_size.y * 0.25)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                for (k, v) in rec.redacted_headers() {
                                    ui.monospace(format!("{k}: {v}"));
                                }
                            });

                        ui.add_space(space::S);
                        ui.separator();
                        ui.strong("Body");
                        ui.horizontal(|ui| {
                            if ui.button("Copy body").clicked() {
                                let text = rec.body_pretty().unwrap_or_default();
                                ctx.copy_text(text);
                                self.status_msg = "Body copied to clipboard".into();
                            }
                        });
                        // Fill all remaining vertical space so the scrollbar
                        // sits at the right edge of the pop-up, not next to
                        // the text.
                        egui::ScrollArea::vertical()
                            .id_salt("capture-body")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                match rec.body_pretty() {
                                    Some(body) => ui.monospace(body),
                                    None => ui.weak("(no body)"),
                                };
                            });
                    });
                });
                if response.backdrop_response.clicked() || response.should_close() {
                    self.selected = None;
                }
            } else {
                self.selected = None;
            }
        }
    }
}

/// Launch the UI and report a clear error when no display is available.
pub fn run(cfg: Config, store: Store, rt: tokio::runtime::Handle) -> anyhow::Result<()> {
    let native = eframe::NativeOptions::default();
    eframe::run_native(
        "JanitorAI Grabber",
        native,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(GrabberApp::new(cfg, store, rt)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("UI failed (is a display available on Linux?): {e}"))
}
