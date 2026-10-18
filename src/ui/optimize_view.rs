//! Full optimization wizard: scope (consents) → analysis → review and approval → run → result.
//! Nothing changes on the machine before the approval on the review step.

use super::widgets::{card, danger_button, heading, primary_button};
use super::{ACCENT, DANGER, MUTED, WARN};
use crate::app::{App, View};
use crate::optimize::{status_text, task_title, OptStep};
use draxbyte::core::docker::{DockerDetail, DockerItem, DockerStatus, Part, Parts};
use draxbyte::core::dupes::{STAGE_FULL, STAGE_INDEX, STAGE_QUICK};
use draxbyte::core::optimize::{self as opt, docker_estimate, part_actionable, Analysis, StepStatus, Task};
use draxbyte::i18n::t;
use draxbyte::models::human_bytes;
use draxbyte::tf;
use egui::RichText;
use std::sync::atomic::Ordering::Relaxed;

fn task_help(task: Task) -> &'static str {
    match task {
        Task::Cleanup => t(
            "Tarama sonucundaki YEŞİL (güvenle silinebilir) öğeleri Çöp Kutusu'na taşır: önbellekler, geçici dosyalar, günlükler. Sarı ve kırmızı öğelere dokunulmaz.",
            "Moves the GREEN (safe to delete) items of a scan to the Recycle Bin: caches, temp files, logs. Yellow and red items are never touched.",
        ),
        Task::Dupes => t(
            "Birebir aynı dosyaları bulur; her gruptan EN ESKİ kopya kalır, fazlalar Çöp Kutusu'na gider. Geliştirme klasörleri (node_modules, .git…) atlanır.",
            "Finds files with exactly the same content; the OLDEST copy of every group stays, the extras go to the Recycle Bin. Developer folders (node_modules, .git…) are skipped.",
        ),
        Task::Docker => t(
            "Docker'ı parça parça temizler: build önbelleği, boşta imajlar, ağlar güvenle gelir; durmuş konteynerler, kullanılmayan imajlar ve birimler ayrıca seçilir (listesi onay adımında görünür). Çalışan konteynerlere dokunulmaz.",
            "Cleans Docker piece by piece: build cache, dangling images and networks come by default; stopped containers, unused images and volumes are opt-in (the list is shown on the approval step). Running containers are never touched.",
        ),
        Task::DockerCompact => t(
            "Docker'ın içinde boşalan yeri Windows'a geri verir (diske gerçek yer ancak bununla açılır). Docker ve TÜM konteynerler birkaç dakika durur; önceden çalışanlar eski sırayla yeniden başlatılır ve hepsinin ayağa kalktığı doğrulanır. İçerideki hiçbir veri silinmez. Yalnızca Windows, yönetici izni gerekir.",
            "Gives the space freed inside Docker back to Windows (this is what really frees disk space). Docker and ALL containers stop for a few minutes; the ones that were running are started again in the old order and checked. Nothing inside is deleted. Windows only, needs administrator rights.",
        ),
        Task::RecycleBin => t(
            "Çöp Kutusu'nu KALICI boşaltır (önceki silinenler dahil). Bu adımdan sonra yukarıdaki adımlar geri alınamaz. Gerçek yer ancak bununla açılır.",
            "Empties the Recycle Bin FOR GOOD (including what you deleted earlier). After this step the steps above cannot be undone. Space is only really freed by this.",
        ),
        Task::Memory => t(
            "Boştaki bellek sayfalarını RAM'den çıkarır. Etki geçicidir; hiçbir program kapatılmaz. Yalnızca Windows.",
            "Moves idle memory pages out of RAM. The effect is temporary; no program is closed. Windows only.",
        ),
    }
}

fn part_title(p: Part) -> &'static str {
    match p {
        Part::StoppedContainers => t("Durmuş konteynerler", "Stopped containers"),
        Part::BuildCache => t("Build önbelleği (kullanılmayanların tümü)", "Build cache (all unused)"),
        Part::DanglingImages => t("Etiketsiz imajlar", "Untagged (dangling) images"),
        Part::UnusedImages => t("Hiçbir konteynerin kullanmadığı tüm imajlar", "All images no container uses"),
        Part::Networks => t("Kullanılmayan ağlar", "Unused networks"),
        Part::Volumes => t("Kullanılmayan birimler (volume)", "Unused volumes"),
    }
}

fn part_help(p: Part) -> &'static str {
    match p {
        Part::StoppedContainers => t("Kapalı konteynerler silinir; birime yazılmamış veriler (ör. konteyner içindeki veritabanı) GERİ GELMEZ.", "Stopped containers are deleted; data not stored in a volume (e.g. a database inside the container) is GONE."),
        Part::BuildCache => t("Yeniden üretilebilir; sonraki build ilk seferde daha yavaş olur.", "Rebuildable; the next build is slower the first time."),
        Part::DanglingImages => t("Etiketi kalmamış eski imaj katmanları; güvenli.", "Old image layers with no tag left; safe."),
        Part::UnusedImages => t("Gerektiğinde yeniden indirilir (zaman ve ağ harcar).", "Downloaded again when needed (takes time and bandwidth)."),
        Part::Networks => t("Hiçbir konteynerin bağlı olmadığı ağlar; güvenli.", "Networks no container is attached to; safe."),
        Part::Volumes => t("Hiçbir konteynere bağlı olmayan birimler silinir; içindeki KALICI veri geri gelmez.", "Volumes no container uses are deleted; the PERSISTENT data inside is gone."),
    }
}

fn part_items(d: &DockerDetail, p: Part) -> &[DockerItem] {
    match p {
        Part::StoppedContainers => &d.stopped,
        Part::DanglingImages => &d.dangling_images,
        Part::Volumes => &d.volumes,
        _ => &[],
    }
}

/// Docker sub-choices. With `d` (review step) each part shows what it would remove and is
/// disabled when there is nothing; without it (scope step) only the choice and its warning.
fn docker_parts_ui(ui: &mut egui::Ui, parts: &mut Parts, d: Option<&DockerDetail>) -> bool {
    let mut changed = false;
    for part in Part::ALL {
        let actionable = d.is_none_or(|d| part_actionable(d, part));
        if !actionable && parts.has(part) {
            parts.set(part, false);
        }
        ui.horizontal_wrapped(|ui| {
            let mut on = parts.has(part);
            ui.add_enabled_ui(actionable, |ui| {
                if ui.checkbox(&mut on, part_title(part)).changed() {
                    parts.set(part, on);
                    changed = true;
                }
            });
            if !actionable {
                ui.label(RichText::new(t("— yok", "— none")).color(MUTED).small());
            }
        });
        ui.label(RichText::new(part_help(part)).color(if part.risky() { WARN } else { MUTED }).small());
        if let Some(d) = d {
            let items = part_items(d, part);
            if actionable && !items.is_empty() {
                let total: u64 = items.iter().map(|i| draxbyte::core::docker::parse_size(i.size.split_whitespace().next().unwrap_or(""))).sum();
                let head = if total > 0 { tf!("{} öğe · toplam {}", "{} items · {} in total", items.len(), human_bytes(total)) } else { tf!("{} öğe", "{} items", items.len()) };
                egui::CollapsingHeader::new(RichText::new(head).small()).id_salt(("docker-part", part as u8)).show(ui, |ui| {
                    for i in items.iter().take(40) {
                        ui.add(egui::Label::new(RichText::new(format!("{} · {} · {}", i.size, i.name, i.info)).small().color(MUTED)).truncate());
                    }
                    if items.len() > 40 {
                        ui.label(RichText::new(tf!("… ve {} tane daha", "… and {} more", items.len() - 40)).small().color(MUTED));
                    }
                });
            }
        }
    }
    changed
}

/// Memory trimming exists on Windows only; elsewhere the choice is shown but disabled.
fn task_available(task: Task) -> bool {
    !matches!(task, Task::Memory | Task::DockerCompact) || cfg!(windows)
}

fn steps_bar(ui: &mut egui::Ui, step: OptStep) {
    let items = [
        (OptStep::Scope, t("1 Kapsam", "1 Scope")),
        (OptStep::Analyzing, t("2 Analiz", "2 Analysis")),
        (OptStep::Review, t("3 Onay", "3 Approve")),
        (OptStep::Running, t("4 Uygulama", "4 Run")),
        (OptStep::Done, t("5 Sonuç", "5 Result")),
    ];
    let cur = items.iter().position(|(s, _)| *s == step).unwrap_or(0);
    ui.horizontal_wrapped(|ui| {
        for (i, (_, label)) in items.iter().enumerate() {
            let color = if i == cur { ACCENT } else if i < cur { super::TEXT } else { MUTED };
            let text = RichText::new(*label).color(color);
            ui.label(if i == cur { text.font(super::bold(14.5)) } else { text });
            if i + 1 < items.len() {
                ui.label(RichText::new("›").color(MUTED));
            }
        }
    });
    ui.add_space(8.0);
}

pub fn optimize(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        t("Tam Optimizasyon", "Full Optimization"),
        t(
            "Önce ne yapılacağını sen seçersin, program ölçer ve sonucu gösterir; sen onaylamadan hiçbir şey değişmez.",
            "You choose what to do, the app measures and shows the result; nothing changes until you approve.",
        ),
    );
    steps_bar(ui, app.opt.step);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match app.opt.step {
        OptStep::Scope => scope_step(app, ui),
        OptStep::Analyzing => analyzing_step(app, ui),
        OptStep::Review => review_step(app, ui),
        OptStep::Running => running_step(app, ui),
        OptStep::Done => done_step(app, ui),
    });
}

// ---- 1: scope ---------------------------------------------------------------------------------

fn scope_step(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    ui.label(RichText::new(t("Neler yapılsın?", "What should be done?")).font(super::bold(16.0)));
    ui.add_space(4.0);
    for task in Task::ALL {
        let available = task_available(task);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut on = app.opt.scope.has(task) && available;
            ui.add_enabled_ui(available, |ui| {
                if ui.checkbox(&mut on, RichText::new(task_title(task)).strong()).changed() {
                    app.opt.scope.set(task, on);
                }
            });
            ui.label(RichText::new(task_help(task)).color(if matches!(task, Task::RecycleBin | Task::DockerCompact) { WARN } else { MUTED }).small());
            if !available {
                ui.label(RichText::new(t("Bu bilgisayarda yok: macOS belleği kendisi yönetir.", "Not available here: macOS manages memory itself.")).color(MUTED).small());
            }
            if on && task == Task::Cleanup {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(t("Taranacak yollar:", "Folders to scan:")).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut app.opt.scan_text).hint_text(t("boş = ayarlardaki varsayılan (tüm sürücüler ve sistem klasörleri)", "empty = the default from Settings (all drives and system folders)")).desired_width(430.0));
                });
            }
            if on && task == Task::Docker {
                ui.add_space(2.0);
                docker_parts_ui(ui, &mut app.opt.scope.docker_parts, None);
            }
            if on && task == Task::Dupes {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(t("Aranacak yollar (; ile ayır):", "Folders to search (separate with ;):")).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut app.opt.dupe_text).desired_width(260.0));
                    ui.label(RichText::new(t("En az", "At least")).color(MUTED));
                    ui.add(egui::DragValue::new(&mut app.opt.scope.dupe_min_mb).range(0..=100_000).suffix(" MB"));
                });
            }
        });
        ui.add_space(4.0);
    }
    if !draxbyte::core::elevate::is_elevated() && cfg!(windows) && app.opt.scope.cleanup {
        ui.label(RichText::new(t("Yönetici izni olmadan bazı sistem klasörleri okunamaz; tarama sonunda ne kadarının okunamadığı gösterilir.", "Without administrator rights some system folders cannot be read; the analysis reports how many.")).color(MUTED).small());
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.add_enabled(app.opt.scope.any(), primary_button(t("Analiz et (hiçbir şey değişmez)", "Analyze (changes nothing)"))).clicked() {
            app.opt_analyze(&ctx);
        }
    });
    ui.label(RichText::new(t("Analiz yalnızca okur. Silme, bir sonraki adımda listeyi gördükten ve onayladıktan sonra yapılır.", "The analysis only reads. Deleting happens after you have seen the list and approved it on the next step.")).color(MUTED).small());
}

// ---- 2: analysis ------------------------------------------------------------------------------

fn analyzing_step(app: &mut App, ui: &mut egui::Ui) {
    let p = app.opt.progress.clone();
    let line = match p.stage.load(Relaxed) {
        opt::STAGE_SCAN => tf!(
            "Sabit sürücüler taranıyor… {} dosya · {}",
            "Scanning… {} files · {}",
            p.scan.files.load(Relaxed),
            human_bytes(p.scan.bytes.load(Relaxed))
        ),
        opt::STAGE_DUPES => match p.dupes.stage.load(Relaxed) {
            s if s == STAGE_INDEX => tf!("Kopya araması: {} dosya taranıyor…", "Duplicate search: indexing {} files…", p.dupes.files_seen.load(Relaxed)),
            s if s == STAGE_QUICK => tf!("Kopya araması: {} aday hızlı kontrol ediliyor…", "Duplicate search: quick-checking {} candidates…", p.dupes.candidates.load(Relaxed)),
            s if s == STAGE_FULL => tf!("Kopya araması: tam karşılaştırma {} / {}", "Duplicate search: full comparison {} / {}", p.dupes.hashed.load(Relaxed), p.dupes.to_hash.load(Relaxed)),
            _ => t("Kopya araması…", "Duplicate search…").to_string(),
        },
        _ => t("Çöp Kutusu, Docker ve bellek bakılıyor…", "Checking the Recycle Bin, Docker and memory…").to_string(),
    };
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new(format!("{line} · {} {}", app.opt.started.elapsed().as_secs(), t("sn", "s"))).color(MUTED));
        });
        ui.label(RichText::new(t("Salt okuma: bu aşamada hiçbir dosya değişmez.", "Read-only: no file is changed at this stage.")).color(ACCENT).small());
        if ui.button(t("İptal", "Cancel")).clicked() {
            app.opt_cancel();
        }
    });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
}

// ---- 3: review --------------------------------------------------------------------------------

/// "what it found" line and the amount, per part.
fn found(an: &Analysis, task: Task, parts: Parts) -> (String, u64, bool) {
    match task {
        Task::Cleanup => (tf!("{} güvenli öğe", "{} safe items", an.cleanup.len()), an.cleanup_bytes, !an.cleanup.is_empty()),
        Task::Dupes => (
            tf!(
                "{} kopya dosya kaldırılacak ({} grup{})",
                "{} duplicate files to remove ({} groups{})",
                an.dupe_delete.len(),
                an.dupe_groups.len().min(opt::DUPE_GROUP_CAP),
                if an.dupe_groups.len() > opt::DUPE_GROUP_CAP { tf!(", en çok yer harcayan {} grup", ", the {} that waste the most", opt::DUPE_GROUP_CAP) } else { String::new() }
            ),
            an.dupe_bytes,
            !an.dupe_delete.is_empty(),
        ),
        Task::Docker => match (&an.docker, &an.docker_state) {
            (Some(d), _) => (
                tf!("{} konteyner durmuş · {} etiketsiz imaj · {} boşta birim", "{} stopped containers · {} untagged images · {} unused volumes", d.stopped.len(), d.dangling_images.len(), d.volumes.len()),
                docker_estimate(d, parts),
                true,
            ),
            (None, Some(DockerStatus::NotInstalled)) => (t("Docker kurulu değil", "Docker is not installed").into(), 0, false),
            (None, Some(DockerStatus::DaemonDown(_))) => (t("Docker çalışmıyor", "Docker is not running").into(), 0, false),
            _ => (t("build önbelleği bulunamadı", "no build cache found").into(), 0, false),
        },
        Task::DockerCompact => match &an.compact {
            Some(c) if c.blocker.is_none() => (tf!("Docker disk imajı {}; geri verilecek yer (tahmini)", "Docker disk image {}; space to give back (estimate)", human_bytes(c.disk_image)), c.estimate, true),
            Some(c) => (c.blocker.clone().unwrap_or_default(), 0, false),
            None => (String::new(), 0, false),
        },
        Task::RecycleBin => match an.bin {
            Some(b) => (tf!("şu an {} öğe", "{} items right now", b.items), b.bytes, true),
            None => (t("okunamadı", "could not be read").into(), 0, false),
        },
        Task::Memory => match an.mem {
            Some(m) => (tf!("şu an boş bellek {} / {}", "free memory right now {} / {}", human_bytes(m.available), human_bytes(m.total)), 0, task_available(task)),
            None => (String::new(), 0, false),
        },
    }
}

fn review_step(app: &mut App, ui: &mut egui::Ui) {
    let Some(an) = app.opt.analysis.clone() else {
        app.opt.step = OptStep::Scope;
        return;
    };
    let ctx = ui.ctx().clone();
    ui.label(RichText::new(t("Bulunanlar — onaylamadan hiçbir şey yapılmaz", "What was found — nothing is done before you approve")).font(super::bold(16.0)));
    ui.add_space(4.0);
    if an.scan_errors > 0 {
        ui.label(RichText::new(tf!("{} yol okunamadı; bunların içindeki dosyalar listede olmayabilir (Tarama ekranında nedeni görünür).", "{} paths could not be read; files inside them may be missing from the list.", an.scan_errors)).color(WARN).small());
    }
    let mut chosen_bytes = 0u64;
    let mut any_task = false;
    for task in Task::ALL {
        if !app.opt.scope.has(task) {
            continue;
        }
        let (what, bytes, actionable) = found(&an, task, app.opt.scope.docker_parts);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut on = actionable && app.opt.scope.has(task);
            ui.add_enabled_ui(actionable, |ui| {
                if ui.checkbox(&mut on, RichText::new(task_title(task)).strong()).changed() {
                    app.opt.scope.set(task, on);
                    app.opt.approved = false;
                }
            });
            if !actionable {
                // nothing to do for this part: make sure it is not part of the run
                app.opt.scope.set(task, false);
            }
            let amount = if bytes > 0 { format!(" · {}", human_bytes(bytes)) } else { String::new() };
            ui.label(RichText::new(format!("{what}{amount}")).color(if actionable { ACCENT } else { MUTED }));
            if actionable && on && task == Task::Docker {
                if let Some(d) = &an.docker {
                    if docker_parts_ui(ui, &mut app.opt.scope.docker_parts, Some(d)) {
                        app.opt.approved = false;
                    }
                    if let Some(img) = d.disk_image {
                        ui.label(RichText::new(tf!(
                            "Docker'ın disk imajı {}. Temizlik Docker'ın İÇİNDE yer açar; dosya kendiliğinden küçülmez (Docker Desktop → Settings → Resources → Disk image ya da WSL'de sıkıştırma gerekir; Docker o sırada kapanır).",
                            "Docker's disk image is {}. The clean-up frees space INSIDE Docker; the file does not shrink by itself (Docker Desktop → Settings → Resources → Disk image, or a WSL compaction, which stops Docker).",
                            human_bytes(img)
                        )).color(MUTED).small());
                    }
                }
            }
            if actionable && on && task == Task::DockerCompact {
                if let Some(c) = &an.compact {
                    ui.label(RichText::new(tf!(
                        "Docker ve şu {} konteyner birkaç dakika durur, sonra eski sırayla yeniden başlatılır: {}",
                        "Docker and these {} containers stop for a few minutes, then are started again in the old order: {}",
                        c.running.len(),
                        c.running.join(", ")
                    )).color(WARN).small());
                    if !c.other_wsl.is_empty() {
                        ui.label(RichText::new(tf!("Çalışan WSL dağıtımları da kapanır: {}", "These running WSL distributions stop too: {}", c.other_wsl.join(", "))).color(DANGER).small());
                    }
                    ui.label(RichText::new(t("Sırasında bu konteynerlere bağlı uygulamalar (veritabanı, API) erişilemez olur. Başlamayan konteyner olursa sonuç ekranı adıyla söyler.", "Apps that depend on these containers (databases, APIs) are unreachable meanwhile. If a container does not start, the result screen names it.")).color(MUTED).small());
                }
            }
            if actionable && matches!(task, Task::Cleanup | Task::Dupes) {
                let sample: Vec<String> = match task {
                    Task::Cleanup => {
                        let mut v: Vec<_> = an.cleanup.iter().collect();
                        v.sort_by(|a, b| b.size.cmp(&a.size));
                        v.iter().take(5).map(|i| format!("{} · {}", human_bytes(i.size), i.path.display())).collect()
                    }
                    _ => an.dupe_delete.iter().take(5).map(|f| format!("{} · {}", human_bytes(f.size), f.path.display())).collect(),
                };
                egui::CollapsingHeader::new(RichText::new(t("En büyük örnekler", "Biggest examples")).small()).id_salt(("opt-sample", task as u8)).show(ui, |ui| {
                    for s in &sample {
                        ui.add(egui::Label::new(RichText::new(s).small().color(MUTED)).truncate());
                    }
                });
            }
        });
        ui.add_space(4.0);
        if app.opt.scope.has(task) && (task != Task::Docker || app.opt.scope.docker_parts.any()) {
            any_task = true;
            // the Recycle Bin line below already counts what the bin holds
            if matches!(task, Task::Cleanup | Task::Dupes | Task::DockerCompact) {
                chosen_bytes += bytes;
            }
        }
    }
    let will_bin = app.opt.scope.recycle_bin;
    if app.opt.scope.recycle_bin {
        if let Some(b) = an.bin {
            // what the earlier steps move there is emptied too
            chosen_bytes += b.bytes;
        }
    }
    if an.startup_enabled > 0 {
        ui.label(RichText::new(tf!("İpucu: {} program Windows'la birlikte açılıyor. Bunları sen karar verirsin (otomatik kapatılmaz): Bellek ve Başlangıç ekranı.", "Hint: {} programs start with Windows. You decide on those (never switched off automatically): Memory & Startup screen.", an.startup_enabled)).color(MUTED).small());
    }
    ui.add_space(6.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(tf!("Tahmini açılacak yer: {}", "Estimated space to free: {}", human_bytes(chosen_bytes))).font(super::bold(16.0)).color(ACCENT));
        ui.label(RichText::new(t("Dosyalar önce Çöp Kutusu'na taşınır (geri alınabilir).", "Files are moved to the Recycle Bin first (can be undone).")).color(MUTED).small());
        if will_bin {
            ui.label(RichText::new(t("Çöp Kutusu boşaltma seçili: bu işlemden sonra geri alma YOKTUR.", "Emptying the Recycle Bin is selected: there is NO undo after that.")).color(DANGER).small());
        }
        ui.checkbox(&mut app.opt.approved, RichText::new(t("Yukarıdaki listeyi inceledim, bu işlemleri onaylıyorum", "I have reviewed the list above and approve these actions")));
        ui.horizontal(|ui| {
            let can = app.opt.approved && any_task;
            let label = t("Optimizasyonu başlat", "Start optimization");
            let btn = if will_bin { danger_button(label) } else { primary_button(label) };
            if ui.add_enabled(can, btn).clicked() {
                app.opt_apply(&ctx);
            }
            if ui.button(t("← Kapsama dön", "← Back to scope")).clicked() {
                app.opt.step = OptStep::Scope;
                app.opt.approved = false;
            }
        });
    });
}

// ---- 4: run -----------------------------------------------------------------------------------

fn running_step(app: &mut App, ui: &mut egui::Ui) {
    let p = app.opt.progress.clone();
    let cur = p.step.load(Relaxed);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(t("Optimizasyon çalışıyor…", "Optimization is running…")).font(super::bold(17.0)));
        for (i, task) in Task::ALL.iter().enumerate() {
            if !app.opt.scope.has(*task) {
                continue;
            }
            let mark = if cur == u8::MAX || (i as u8) > cur { "○" } else if i as u8 == cur { "▶" } else { "✓" };
            ui.label(RichText::new(format!("{mark}  {}", task_title(*task))).color(if i as u8 == cur { ACCENT } else { MUTED }));
        }
        let (d, tot) = (p.clean_done.load(Relaxed), p.clean_total.load(Relaxed));
        if tot > 0 && matches!(Task::ALL.get(cur as usize), Some(Task::Cleanup | Task::Dupes)) {
            ui.add(egui::ProgressBar::new(d as f32 / tot.max(1) as f32).text(format!("{d}/{tot}")).fill(ACCENT));
        }
        if let Ok(n) = p.note.lock() {
            if !n.is_empty() {
                ui.label(RichText::new(n.as_str()).color(WARN));
            }
        }
        ui.label(RichText::new(tf!("Şimdiye dek açılan: {} · {} sn", "Freed so far: {} · {} s", human_bytes(p.freed.load(Relaxed)), app.opt.started.elapsed().as_secs())).color(MUTED));
        if ui.button(t("İptal (kalan adımlar atlanır)", "Cancel (remaining steps are skipped)")).clicked() {
            app.opt_cancel();
        }
    });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
}

// ---- 5: result --------------------------------------------------------------------------------

fn done_step(app: &mut App, ui: &mut egui::Ui) {
    let Some(rep) = &app.opt.report else {
        app.opt.step = OptStep::Scope;
        return;
    };
    let gained = rep.free_after.saturating_sub(rep.free_before);
    let bin_emptied = rep.steps.iter().any(|s| s.task == Task::RecycleBin && s.status == StepStatus::Done);
    let has_undo = !bin_emptied && rep.clean_reports.iter().any(|c| c.entries.iter().any(|e| e.outcome == draxbyte::models::Outcome::Trashed));
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(tf!("Tamamlandı · {} kaldırıldı", "Finished · {} removed", human_bytes(rep.freed()))).font(super::bold(18.0)).color(ACCENT));
        if rep.free_before > 0 {
            ui.label(RichText::new(tf!("Diskte boş yer: {} → {} ({})", "Free disk space: {} → {} ({})", human_bytes(rep.free_before), human_bytes(rep.free_after), if rep.free_after >= rep.free_before { format!("+{}", human_bytes(gained)) } else { t("değişmedi/azaldı (başka işlemler yazmış olabilir)", "unchanged/lower (other processes may have written)").into() })).color(MUTED));
        }
        ui.add_space(4.0);
        for s in &rep.steps {
            let ok = s.status == StepStatus::Done;
            let color = match s.status {
                StepStatus::Done => ACCENT,
                StepStatus::Skipped(_) => MUTED,
                StepStatus::Failed(_) => DANGER,
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(if ok { "✓" } else { "•" }).color(color));
                ui.label(RichText::new(task_title(s.task)).strong());
                let amount = if s.freed > 0 && s.task != Task::Memory { format!(" · {}", human_bytes(s.freed)) } else if s.freed > 0 { format!(" · +{} {}", human_bytes(s.freed), t("boş RAM", "free RAM")) } else { String::new() };
                let detail = if s.detail.is_empty() { String::new() } else { format!(" · {}", s.detail) };
                ui.label(RichText::new(format!("{}{amount}{detail}", status_text(&s.status))).color(color));
            });
        }
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if has_undo && ui.button(t("Geri al (Çöp Kutusu'ndan)", "Undo (from the Recycle Bin)")).clicked() {
            app.opt_undo();
        }
        if ui.button(t("Başlangıç programlarına bak", "Review startup programs")).clicked() {
            app.view = View::System;
        }
        if ui.button(t("Kullanılmayan programlara bak", "Review unused programs")).clicked() {
            app.view = View::Programs;
        }
        if ui.add(primary_button(t("Yeniden başla", "Start over"))).clicked() {
            app.opt_restart();
        }
    });
    ui.label(RichText::new(t("Programlar ve başlangıç girdileri bilerek otomatik değiştirilmez: ne kaldırılacağına sen karar verirsin.", "Programs and startup entries are deliberately not changed automatically: you decide what to remove.")).color(MUTED).small());
}
