use crate::config::Config;
use crate::proxy::ProxyHandle;
use crate::store::model::CaptureRecord;
use crate::store::{export::JsonlWriter, Store};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

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
        }
    }

    fn start_proxy(&mut self) {
        let upstream = self.cfg.upstream_base_url.clone();
        let listen = self.cfg.listen_addr.clone();
        let store = self.store.clone();
        self.status_msg = "Starting…".into();

        let handle_slot: Arc<Mutex<Option<ProxyHandle>>> = Arc::new(Mutex::new(None));
        self.state = ProxyState::Starting;

        let slot = handle_slot.clone();
        self.rt.spawn(async move {
            match crate::proxy::server::spawn(&listen, upstream, store).await {
                Ok(h) => {
                    tracing::info!(addr = %h.addr, "proxy started");
                    *slot.lock().await = Some(h);
                }
                Err(e) => tracing::error!(error = %e, "proxy failed to start"),
            }
        });
        self.state = ProxyState::Running(handle_slot);
        self.status_msg = format!("Running on {}", self.cfg.listen_addr);
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
                self.status_msg = format!(
                    "Exported {ok} records to {}",
                    w.path().display()
                );
            }
            Err(e) => self.status_msg = format!("Export failed: {e}"),
        }
    }
}

impl eframe::App for GrabberApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let running = matches!(self.state, ProxyState::Running(_));
                let dot = if running {
                    egui::RichText::new("●").color(egui::Color32::GREEN)
                } else {
                    egui::RichText::new("●").color(egui::Color32::RED)
                };
                ui.label(dot);
                ui.label(&self.status_msg);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if running {
                        if ui.button("Stop").clicked() {
                            self.stop_proxy();
                        }
                    } else if ui.button("Start").clicked() {
                        self.start_proxy();
                    }
                    if ui.button("Refresh").clicked() {
                        self.refresh_captures();
                    }
                    if ui.button("Export JSONL").clicked() {
                        self.export_jsonl();
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Captured LLM traffic");
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("captures")
                    .striped(true)
                    .num_columns(5)
                    .show(ui, |ui| {
                        ui.strong("Time");
                        ui.strong("Dir");
                        ui.strong("Path");
                        ui.strong("Status");
                        ui.strong("Body");
                        ui.end_row();

                        for rec in &self.captures {
                            ui.label(rec.timestamp.format("%H:%M:%S").to_string());
                            ui.label(match rec.direction {
                                crate::store::Direction::Request => "→ req",
                                crate::store::Direction::Response => "← resp",
                            });
                            ui.monospace(&rec.path);
                            ui.label(
                                rec.status
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| "—".into()),
                            );
                            if ui.button("View").clicked() {
                                self.selected = Some(rec.id);
                            }
                            ui.end_row();
                        }
                    });
            });
        });

        if let Some(id) = self.selected {
            if let Some(rec) = self.captures.iter().find(|c| c.id == id) {
                let mut open = true;
                egui::Window::new(format!("Capture {} {}", rec.method, rec.path))
                    .open(&mut open)
                    .default_width(640.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(match rec.direction {
                                crate::store::Direction::Request => "→ request",
                                crate::store::Direction::Response => "← response",
                            });
                            if rec.secret == crate::store::SecretFlag::Secret {
                                ui.label(egui::RichText::new("contains secrets").color(egui::Color32::YELLOW));
                            }
                        });

                        ui.separator();
                        ui.strong("Headers (secrets redacted)");
                        egui::ScrollArea::vertical()
                            .id_salt("capture-headers")
                            .max_height(140.0)
                            .show(ui, |ui| {
                                for (k, v) in rec.redacted_headers() {
                                    ui.monospace(format!("{k}: {v}"));
                                }
                            });

                        ui.separator();
                        ui.strong("Body");
                        egui::ScrollArea::vertical()
                            .id_salt("capture-body")
                            .show(ui, |ui| {
                                match rec.body_pretty() {
                                    Some(body) => ui.monospace(body),
                                    None => ui.weak("(no body)"),
                                };
                            });
                    });
                if !open {
                    self.selected = None;
                }
            } else {
                self.selected = None;
            }
        }
    }
}

/// Launch the UI. Returns an error with a clear message when no display is available.
pub fn run(cfg: Config, store: Store, rt: tokio::runtime::Handle) -> anyhow::Result<()> {
    let native = eframe::NativeOptions::default();
    eframe::run_native(
        "JanitorAI Grabber",
        native,
        Box::new(move |cc| {
            egui_extras_install(cc);
            Ok(Box::new(GrabberApp::new(cfg, store, rt)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("UI failed (is a display available on Linux?): {e}"))
}

fn egui_extras_install(_cc: &eframe::CreationContext<'_>) {
    // Hook for fonts/theme setup.
}
