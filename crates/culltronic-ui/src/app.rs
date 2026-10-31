//! Culltronic application shell: state machine root, event pump, theming, fonts.

use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use eframe::egui;

use culltronic_core::Cache;
use culltronic_core::Db;
use culltronic_core::IngestEvent;
use culltronic_core::IngestPipeline;
use culltronic_core::PhotoEntry;
use culltronic_core::PhotoId;
use culltronic_core::PhotoStatus;
use culltronic_core::ScanOptions;

use crate::tex::Textures;
use crate::views::Action;
use crate::views::grid::GridView;
use crate::views::home::HomeView;
use crate::views::modals;
use crate::views::modals::Modals;
use crate::views::widgets;

/// Repaint cadence while background work (scan, ingest, decode) is
/// outstanding; short enough that spinners and progressive fills look live.
const FRAME_PULSE: Duration = Duration::from_millis(33);

/// Events produced by background jobs and drained by the App every frame.
enum Event {
    /// Filesystem validation of a drop completed off the UI thread.
    DropPrepared {
        generation: u64,
        result: Result<crate::drops::DropRequest, String>,
    },
    /// A folder scan finished (successfully or not).
    ScanFinished {
        /// Unique scan identity, including repeated opens of the same folder.
        generation: u64,
        /// Root the job was launched for; used to drop stale results.
        root: PathBuf,
        /// Ordered index entries, or a user-facing error message.
        result: Result<Vec<PhotoEntry>, String>,
    },
    /// An export run advanced by one file.
    ExportProgress {
        /// Root whose grid owns the run; stale ticks are dropped.
        root: PathBuf,
        /// Files processed so far, out of the announced total.
        done: usize,
    },
    /// An export run finished (cancelled runs report through the report).
    ExportFinished {
        /// Root whose grid owns the run; stale reports are dropped.
        root: PathBuf,
        /// Copy outcome, or a user-facing message for destination-level
        /// failures (missing / non-directory destination).
        result: Result<culltronic_core::ExportReport, String>,
    },
}

/// A running export job's identity and cancellation switch.
struct ExportJob {
    /// Folder the exported files were scanned from, for staleness checks.
    root: PathBuf,
    /// Polled by the worker before every file copy.
    cancel: Arc<AtomicBool>,
}

/// Export queued behind an overwrite confirmation: the picked destination
/// already holds files with these names, and exporting replaces them.
struct PendingExport {
    /// Folder the exported files were scanned from.
    root: PathBuf,
    /// Absolute paths of the originals to copy.
    files: Vec<PathBuf>,
    /// User-picked destination folder.
    dest: PathBuf,
    /// Source file names already present in `dest`.
    duplicates: Vec<String>,
}

/// Which screen is currently mounted. The grid state is boxed: it is by
/// far the largest variant and swapping screens must stay cheap.
enum Screen {
    Home(HomeView),
    Grid(Box<GridView>),
}

/// Root of the UI state machine; owns views and pumps core events per frame.
pub struct App {
    package_verification_frames: Option<u8>,
    scan_generation: u64,
    preparing_drop: bool,
    pending_drop: Option<Vec<PathBuf>>,
    drop_note: Option<String>,
    db: Arc<Db>,
    screen: Screen,
    events_tx: crossbeam_channel::Sender<Event>,
    events_rx: crossbeam_channel::Receiver<Event>,
    /// Root whose scan is in flight; results for anything else are stale
    /// (e.g. the user backed out mid-scan) and get dropped.
    scanning: Option<PathBuf>,
    /// Ingest engine; `None` only when the platform has no cache directory,
    /// in which case tiles stay placeholders (graceful degradation).
    pipeline: Option<IngestPipeline>,
    ingest_rx: Option<crossbeam_channel::Receiver<IngestEvent>>,
    /// Generation of the batch currently mounted on the grid; events from
    /// any other generation belong to a superseded folder and are dropped.
    ingest_generation: u64,
    /// Thumbnail decode + GPU upload service feeding grid cells.
    textures: Textures,
    /// Help overlay + About dialog (SPEC §10 T14); drawn above whatever
    /// screen is mounted, which stays suspended while either is open.
    modals: Modals,
    /// Export run in flight, if any; cancelled when the folder changes.
    export_job: Option<ExportJob>,
    /// Export waiting for the user to confirm overwriting duplicates in
    /// the picked destination folder, if any.
    pending_export: Option<PendingExport>,
}

impl App {
    /// Configures fonts and theme before the first frame is painted.
    pub fn new(cc: &eframe::CreationContext<'_>, db: Arc<Db>) -> Self {
        install_fonts(&cc.egui_ctx);
        install_theme(&cc.egui_ctx);
        let (events_tx, events_rx) = crossbeam_channel::unbounded();
        let pipeline = match Cache::system_default() {
            Ok(cache) => {
                let (pipeline, ingest_rx) = IngestPipeline::new(cache, Arc::clone(&db));
                Some((pipeline, ingest_rx))
            }
            Err(error) => {
                tracing::warn!(%error, "no cache directory; preview ingestion disabled");
                None
            }
        };
        let (pipeline, ingest_rx) = pipeline
            .map(|(p, rx)| (Some(p), Some(rx)))
            .unwrap_or_else(|| (None, None));
        Self {
            package_verification_frames: None,
            scan_generation: 0,
            preparing_drop: false,
            pending_drop: None,
            drop_note: None,
            db,
            screen: Screen::Home(HomeView),
            events_tx,
            events_rx,
            scanning: None,
            pipeline,
            ingest_rx,
            ingest_generation: 0,
            textures: Textures::new(),
            modals: Modals::default(),
            export_job: None,
            pending_export: None,
        }
    }

    /// Close after rendered UI frames, verifying renderer startup in packaged CI runs.
    pub(crate) fn verify_package(&mut self) {
        self.package_verification_frames = Some(3);
    }

    /// Cancels any in-flight export job; used when the browsed folder is
    /// about to change so a superseded run stops touching disk.
    fn cancel_export(&mut self) {
        if let Some(job) = self.export_job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Mounts the grid immediately and starts a background scan+sync so
    /// placeholders appear without waiting (SPEC §5.1); any ingest still
    /// running for the previous folder is cancelled up front.
    pub(crate) fn open_folder(&mut self, root: PathBuf) {
        self.preparing_drop = false;
        self.pending_drop = None;
        self.drop_note = None;
        self.mount_folder(&root);
        self.scan_folder(root);
    }

    fn mount_folder(&mut self, root: &Path) {
        tracing::info!(?root, "opening folder");
        if let Some(pipeline) = &self.pipeline {
            pipeline.cancel();
        }
        self.cancel_export();
        self.pending_export = None;
        let auto_advance = widgets::load_auto_advance(&self.db);
        let export_mode = widgets::load_export_mode(&self.db);
        self.screen = Screen::Grid(Box::new(GridView::new(
            root.to_owned(),
            auto_advance,
            export_mode,
        )));
    }

    fn scan_folder(&mut self, root: PathBuf) {
        self.scan_generation += 1;
        let generation = self.scan_generation;
        self.scanning = Some(root.clone());
        let db = Arc::clone(&self.db);
        let sender = self.events_tx.clone();
        // Scan is a cheap walk + tiny upserts (< 300 ms @ 10k per SPEC §8);
        // a plain thread suffices — extraction itself runs on the rayon pool.
        std::thread::spawn(move || {
            let result = scan_and_sync(&db, &root);
            let _sent = sender.send(Event::ScanFinished {
                generation,
                root,
                result,
            });
        });
    }

    /// Feeds the freshly synced folder's pending photos to the ingest
    /// pipeline and tells the grid a new batch started.
    fn start_ingest(&mut self, root: &Path) {
        let Some(pipeline) = &self.pipeline else {
            return;
        };
        let pending = match self.db.pending_photos(root) {
            Ok(pending) => pending,
            Err(error) => {
                tracing::warn!(%error, "cannot list pending photos");
                return;
            }
        };
        if pending.is_empty() {
            return;
        }
        let total = pending.len();
        self.ingest_generation = pipeline.enqueue(pending);
        if let Screen::Grid(grid) = &mut self.screen {
            grid.begin_ingest(total);
        }
    }

    /// Applies finished background jobs to the mounted screen.
    fn drain_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                Event::DropPrepared { generation, result } => {
                    if !self.preparing_drop || generation != self.scan_generation {
                        continue;
                    }
                    self.preparing_drop = false;
                    match result {
                        Ok(request) => {
                            self.pending_drop = Some(request.files);
                            self.scan_folder(request.root);
                        }
                        Err(message) => self.drop_note = Some(message),
                    }
                }
                Event::ScanFinished {
                    generation,
                    root,
                    result,
                } => {
                    if self.scan_generation != generation || self.scanning.as_ref() != Some(&root) {
                        continue;
                    }
                    self.scanning = None;
                    let mut dropped_ids = Vec::new();
                    if let Some(files) = self.pending_drop.take() {
                        let resolved = result
                            .as_ref()
                            .map_err(Clone::clone)
                            .and_then(|entries| crate::drops::photo_ids(entries, &files));
                        match resolved {
                            Ok(ids) => {
                                dropped_ids = ids;
                                self.mount_folder(&root);
                                self.drop_note = None;
                            }
                            Err(message) => {
                                self.drop_note = Some(message);
                                continue;
                            }
                        }
                    }
                    match (&mut self.screen, result) {
                        (Screen::Grid(grid), outcome) => {
                            // Purge texture state from any other folder before the
                            // sheet mounts: cancelled batches stop occupying decode
                            // and GPU budget, while this folder's warm cache stays.
                            if let Ok(entries) = &outcome {
                                let keep: HashSet<PhotoId> = entries.iter().map(|e| e.id).collect();
                                self.textures.retain(&keep);
                            }
                            grid.apply_scan(outcome);
                            grid.select_dropped(&dropped_ids);
                            self.start_ingest(&root);
                        }
                        (Screen::Home(_), _) => {}
                    }
                }
                Event::ExportProgress { root, done } => {
                    if let (Screen::Grid(grid), Some(_)) = (&mut self.screen, &self.export_job)
                        && grid.is_browsing(&root)
                    {
                        grid.apply_export_progress(done);
                    }
                }
                Event::ExportFinished { root, result } => {
                    if self.export_job.as_ref().is_none_or(|job| job.root != root) {
                        continue;
                    }
                    self.cancel_export();
                    if let Screen::Grid(grid) = &mut self.screen
                        && grid.is_browsing(&root)
                    {
                        grid.finish_export(&result);
                    }
                }
            }
        }
        if let Some(rx) = &self.ingest_rx {
            // Two-phase drain: the receiver borrows `self`, so events are
            // collected before they are applied mutably.
            let mut arrived = Vec::new();
            while let Ok(event) = rx.try_recv() {
                arrived.push(event);
            }
            for event in arrived {
                self.apply_ingest_event(event);
            }
        }
    }

    /// Routes one pipeline event into the grid; events from a superseded
    /// batch (folder switched mid-run) are dropped by generation. Each
    /// result re-reads its index row so cells gain thumb path, pixel size
    /// and error messages exactly when ingest produced them.
    fn apply_ingest_event(&mut self, event: IngestEvent) {
        let (id, status) = match event {
            IngestEvent::Ingested(id) => (id, PhotoStatus::Ok),
            IngestEvent::Failed(id) => (id, PhotoStatus::Error),
            IngestEvent::Finished { generation } => {
                if generation == self.ingest_generation
                    && let Screen::Grid(grid) = &mut self.screen
                {
                    grid.finish_ingest();
                }
                return;
            }
        };
        let fresh = self.lookup_entry(id);
        if let Screen::Grid(grid) = &mut self.screen {
            grid.apply_ingest_result(id, status, fresh);
        }
    }

    /// Best-effort row refresh for one ingest event; failures degrade to a
    /// status-only cell update rather than dropping the event.
    fn lookup_entry(&self, id: culltronic_core::PhotoId) -> Option<PhotoEntry> {
        match self.db.photo_entry(id) {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(%error, ?id, "cannot refresh ingested photo");
                None
            }
        }
    }

    /// Forwards changed visible-pending sets to the ingest queue so tiles
    /// near the viewport extract first (SPEC §5.2 priority ping).
    fn pump_priority(&mut self) {
        let Screen::Grid(grid) = &mut self.screen else {
            return;
        };
        if let Some(ids) = grid.take_priority_ping()
            && let Some(pipeline) = &self.pipeline
        {
            pipeline.prioritize(&ids);
        }
    }

    /// Keeps the frame loop alive only while background work is outstanding;
    /// idle screens cost zero wakeups.
    fn schedule_repaint(&self, ctx: &egui::Context) {
        let busy = self.scanning.is_some()
            || self.preparing_drop
            || matches!(&self.screen, Screen::Grid(grid) if grid.is_ingesting() || grid.is_exporting())
            || self.textures.busy();
        if busy {
            ctx.request_repaint_after(FRAME_PULSE);
        }
    }

    /// Executes one action produced by the active view.
    fn run_action(&mut self, action: Option<Action>) {
        match action {
            None => {}
            Some(Action::PickFolder) => {
                // Blocking native dialog; cancelled picks are a no-op.
                if let Some(picked) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder(picked);
                }
            }
            Some(Action::OpenFolder(path)) => self.open_folder(path),
            Some(Action::Export { root, files }) => {
                // Blocking native dialog; a cancelled pick is a no-op.
                let Some(dest) = rfd::FileDialog::new().pick_folder() else {
                    return;
                };
                // `export_files` overwrites like `cp`; pause for a
                // confirmation when that would silently replace files
                // already sitting in the destination.
                let duplicates = culltronic_core::existing_names(&files, &dest);
                if duplicates.is_empty() {
                    self.start_export(root, files, dest);
                } else {
                    self.pending_export = Some(PendingExport {
                        root,
                        files,
                        dest,
                        duplicates,
                    });
                }
            }
            Some(Action::BackToHome) => {
                self.scanning = None;
                self.preparing_drop = false;
                self.pending_drop = None;
                if let Some(pipeline) = &self.pipeline {
                    pipeline.cancel();
                }
                self.cancel_export();
                self.screen = Screen::Home(HomeView);
            }
            Some(Action::ShowAbout) => self.modals.open_about(),
            Some(Action::ClearRecentFolders) => {
                if let Err(error) = self.db.clear_recent_roots() {
                    self.drop_note = Some(format!("Could not clear folder history: {error}"));
                }
            }
        }
    }

    /// Launches a copy run for an already-picked destination: registers
    /// the job, flips the grid into progress mode and spawns the worker.
    fn start_export(&mut self, root: PathBuf, files: Vec<PathBuf>, dest: PathBuf) {
        let total = files.len();
        let cancel = Arc::new(AtomicBool::new(false));
        self.export_job = Some(ExportJob {
            root: root.clone(),
            cancel: Arc::clone(&cancel),
        });
        if let Screen::Grid(grid) = &mut self.screen {
            grid.begin_export(total);
        }
        let sender = self.events_tx.clone();
        tracing::info!(count = total, ?root, ?dest, "export started");
        std::thread::spawn(move || {
            let result = culltronic_core::export_files(&files, &dest, &cancel, |done| {
                let _sent = sender.send(Event::ExportProgress {
                    root: root.clone(),
                    done,
                });
            })
            .map_err(|error| error.to_string());
            let _sent = sender.send(Event::ExportFinished { root, result });
        });
    }

    /// Draws the overwrite-confirmation dialog while an export is pending
    /// and resolves its outcome: confirming (button or Enter) starts the
    /// copy run; dismissing (Cancel button, Esc or backdrop click) drops it.
    /// Runs after the screens so the chrome paints on top of everything,
    /// mirroring [`Modals::draw`].
    fn pump_export_confirm(&mut self, ctx: &egui::Context) {
        use crate::theme::{ACCENT, BG, MUTED, TEXT};

        let Some(pending) = self.pending_export.as_ref() else {
            return;
        };
        let (escape, enter) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::Enter),
            )
        });
        let folder_name = pending.dest.file_name().map_or_else(
            || pending.dest.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let count = pending.duplicates.len();
        let names_preview = match count {
            0..=4 => pending.duplicates.join(", "),
            _ => format!(
                "{}, … and {} more",
                pending.duplicates[..4].join(", "),
                count - 4
            ),
        };
        let noun = if count == 1 { "file" } else { "files" };

        let mut confirmed = false;
        let dismissed =
            modals::draw_dialog(ctx, egui::Id::new("culltronic_export_confirm"), |ui| {
                ui.label(
                    egui::RichText::new("Overwrite existing files?")
                        .heading()
                        .strong()
                        .color(TEXT),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("The folder").color(TEXT));
                    ui.label(egui::RichText::new(folder_name).strong().color(ACCENT));
                    ui.label(
                        egui::RichText::new(format!(
                            "already contains {count} {noun} with these names:"
                        ))
                        .color(TEXT),
                    );
                });
                ui.label(egui::RichText::new(names_preview).color(MUTED));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Exporting will overwrite them with your copies of these {noun}."
                    ))
                    .color(TEXT),
                );
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(format!("Overwrite {count} {noun}")).color(BG),
                            )
                            .fill(ACCENT),
                        )
                        .clicked()
                    {
                        confirmed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        return true;
                    }
                    false
                })
                .inner
            });

        if confirmed || enter {
            if let Some(job) = self.pending_export.take() {
                self.start_export(job.root, job.files, job.dest);
            }
        } else if escape || dismissed {
            self.pending_export = None;
        }
    }
}

impl App {
    /// Retire slow drop work while keeping the current folder and selection.
    fn cancel_pending_drop(&mut self) {
        self.scan_generation += 1;
        self.preparing_drop = false;
        self.pending_drop = None;
        self.scanning = None;
        self.drop_note = None;
    }

    /// Reject drops during modal/export work; never cancel a copy by accident.
    fn pump_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input_mut(|input| std::mem::take(&mut input.raw.dropped_files));
        if dropped.is_empty() {
            return;
        }
        if self.modals.any() || self.pending_export.is_some() || self.export_job.is_some() {
            self.drop_note = Some(
                "Finish or cancel the current dialog or export before dropping photos.".into(),
            );
            return;
        }
        if self.scanning.is_some() && self.pending_drop.is_none() {
            self.drop_note = Some(
                "Wait for the current folder to finish opening before dropping photos.".into(),
            );
            return;
        }
        let paths = dropped.iter().map(|file| file.path().to_owned()).collect();
        // Metadata on an unavailable drive can block: do all drop I/O off-thread.
        self.scan_generation += 1;
        let generation = self.scan_generation;
        self.preparing_drop = true;
        self.pending_drop = None;
        self.scanning = None;
        self.drop_note = Some("Opening dropped photos…".into());
        let sender = self.events_tx.clone();
        std::thread::spawn(move || {
            let result = crate::drops::resolve(paths);
            let _sent = sender.send(Event::DropPrepared { generation, result });
        });
    }

    fn draw_drop_feedback(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| !input.raw.hovered_files.is_empty()) {
            let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drop_hint"));
            let painter = ctx.layer_painter(layer);
            let rect = ctx.content_rect();
            painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(210));
            painter.text(rect.center(), egui::Align2::CENTER_CENTER,
                "Drop one folder or RAW photos from one folder\nCompanion JPEGs select their RAW+JPEG pair",
                egui::FontId::proportional(22.0), egui::Color32::WHITE);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Upload freshly decoded thumbnails before drawing so cells can
        // paint them this same frame.
        self.textures.sync(&ctx);
        self.drain_events();
        self.pump_drops(&ctx);
        if let Some(note) = self.drop_note.clone() {
            egui::Panel::top("drop_status").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(note);
                    if self.preparing_drop || self.pending_drop.is_some() {
                        if ui.small_button("Cancel").clicked() {
                            self.cancel_pending_drop();
                        }
                    } else if ui.small_button("Dismiss").clicked() {
                        self.drop_note = None;
                    }
                });
            });
        }
        // Modal open/close intents are read before the views so a closing
        // keypress never falls through into the screen underneath; while
        // a modal or the export confirmation is up the views render but
        // stay input-suspended.
        self.modals.pump_input(&ctx);
        let suspended = self.modals.any()
            || self.pending_export.is_some()
            || self.pending_drop.is_some()
            || self.preparing_drop;

        match &mut self.screen {
            Screen::Home(home) => {
                let mut action = None;
                egui::CentralPanel::default().show(ui, |ui| {
                    action = home.ui(ui, &self.db);
                });
                if !suspended {
                    self.run_action(action);
                }
            }
            Screen::Grid(grid) => {
                let mut action = None;
                egui::CentralPanel::default().show(ui, |ui| {
                    action = grid.ui(ui, &self.db, &mut self.textures, suspended);
                });
                if !suspended {
                    self.run_action(action);
                }
            }
        }

        // Dialogs paint last so they sit above every layer the screens
        // created this frame (SPEC §6 `?` overlay, About, export confirm).
        self.pump_export_confirm(&ctx);
        self.modals.draw(&ctx);
        self.draw_drop_feedback(&ctx);

        self.pump_priority();
        self.schedule_repaint(&ctx);
        if let Some(frames) = &mut self.package_verification_frames {
            *frames -= 1;
            if *frames == 0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.package_verification_frames = None;
            } else {
                ctx.request_repaint();
            }
        }
    }
}

/// One scan pass over a folder: filesystem walk, index sync and recents
/// stamp. Runs off the UI thread; failures become user-facing strings.
fn scan_and_sync(db: &Db, root: &Path) -> Result<Vec<PhotoEntry>, String> {
    let scanned = culltronic_core::scan_folder(root, ScanOptions::default())
        .map_err(|error| error.to_string())?;
    let entries = db
        .sync_scan(root, &scanned, ScanOptions::default())
        .map_err(|error| error.to_string())?;
    db.touch_root(root, culltronic_core::now_millis())
        .map_err(|error| error.to_string())?;
    tracing::info!(count = entries.len(), ?root, "folder synced");
    Ok(entries)
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let inter = egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    fonts.font_data.insert("inter_regular".into(), inter.into());
    // Inter first, stock egui fonts as fallback for glyphs Inter lacks.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "inter_regular".into());
    #[cfg(windows)]
    install_windows_font_fallbacks(&mut fonts);
    ctx.set_fonts(fonts);
}

/// Use installed fonts for non-Latin filenames without redistributing system fonts.
#[cfg(windows)]
fn install_windows_font_fallbacks(fonts: &mut egui::FontDefinitions) {
    let Some(windows) = std::env::var_os("SystemRoot") else {
        return;
    };
    let directory = PathBuf::from(windows).join("Fonts");
    for name in [
        "msyh.ttc",
        "msjh.ttc",
        "YuGothR.ttc",
        "malgun.ttf",
        "segoeui.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(directory.join(name)) {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.into());
            }
        }
    }
}

fn install_theme(ctx: &egui::Context) {
    use crate::theme::{ACCENT, BG, MUTED, PANEL, TEXT};

    // Global style so both light/dark theme variants share our palette;
    // the app is dark-only by design.
    ctx.global_style_mut(|style| {
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.panel_fill = PANEL;
        v.window_fill = PANEL;
        v.extreme_bg_color = BG;
        v.faint_bg_color = BG;
        v.selection.bg_fill = ACCENT;
        v.selection.stroke.color = BG;
        v.widgets.inactive.fg_stroke.color = TEXT;
        v.widgets.hovered.fg_stroke.color = ACCENT;
        v.widgets.active.fg_stroke.color = ACCENT;
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
        );
        // Secondary text (hints, tile strips) uses the muted gray.
        v.weak_text_color = Some(MUTED);
    });
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used)]

    use super::*;

    fn test_app(db: Db) -> App {
        let (events_tx, events_rx) = crossbeam_channel::unbounded();
        App {
            package_verification_frames: None,
            scan_generation: 0,
            preparing_drop: false,
            pending_drop: None,
            drop_note: None,
            db: Arc::new(db),
            screen: Screen::Home(HomeView),
            events_tx,
            events_rx,
            scanning: None,
            pipeline: None,
            ingest_rx: None,
            ingest_generation: 0,
            textures: Textures::new(),
            modals: Modals::default(),
            export_job: None,
            pending_export: None,
        }
    }

    #[test]
    fn stale_same_folder_scan_cannot_consume_the_latest_drop() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        let mut app = test_app(Db::open(&dir.path().join("index.db")).unwrap());
        app.scan_generation = 2;
        app.scanning = Some(root.clone());
        app.pending_drop = Some(Vec::new());
        app.events_tx
            .send(Event::ScanFinished {
                generation: 1,
                root: root.clone(),
                result: Ok(Vec::new()),
            })
            .unwrap();
        app.drain_events();
        assert!(matches!(app.screen, Screen::Home(_)));
        assert!(app.pending_drop.is_some());
        app.events_tx
            .send(Event::ScanFinished {
                generation: 2,
                root: root.clone(),
                result: Ok(Vec::new()),
            })
            .unwrap();
        app.drain_events();
        assert!(matches!(&app.screen, Screen::Grid(grid) if grid.is_browsing(&root)));
        assert!(app.pending_drop.is_none());
    }

    #[test]
    fn invalid_drop_or_scan_error_preserves_the_mounted_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = test_app(Db::open(&dir.path().join("index.db")).unwrap());
        let original = dir.path().join("original");
        let dropped = dir.path().join("dropped");
        app.mount_folder(&original);
        for result in [Ok(Vec::new()), Err("Access denied".into())] {
            app.scanning = Some(dropped.clone());
            app.pending_drop = Some(vec!["unpaired.JPG".into()]);
            app.events_tx
                .send(Event::ScanFinished {
                    generation: 0,
                    root: dropped.clone(),
                    result,
                })
                .unwrap();
            app.drain_events();
            assert!(matches!(&app.screen, Screen::Grid(grid) if grid.is_browsing(&original)));
            assert!(app.drop_note.is_some());
        }
    }

    #[test]
    fn cancelled_drop_ignores_late_filesystem_validation() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = test_app(Db::open(&dir.path().join("index.db")).unwrap());
        let original = dir.path().join("original");
        app.mount_folder(&original);
        app.preparing_drop = true;
        app.cancel_pending_drop();
        app.events_tx
            .send(Event::DropPrepared {
                generation: 0,
                result: Ok(crate::drops::DropRequest {
                    root: dir.path().join("late drop"),
                    files: Vec::new(),
                }),
            })
            .unwrap();
        app.drain_events();
        assert!(matches!(&app.screen, Screen::Grid(grid) if grid.is_browsing(&original)));
        assert!(app.scanning.is_none());
        assert!(!app.preparing_drop);
    }
}
