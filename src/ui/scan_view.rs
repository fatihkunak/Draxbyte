//! Home screen and the live scan-progress screen.

use super::widgets::{
    action_card, card, category_color, donut, heading, kind_badge, kind_color, primary_button,
    ring, stat_fill, Icon,
};
use super::{ACCENT, BLUE, MUTED, WARN};
use crate::app::{App, Confirm, SortKey, View};
use draxbyte::i18n::t;
use draxbyte::models::{human_bytes, now_secs, Category, Kind};
use draxbyte::tf;
use egui::RichText;
use egui_extras::{Size, StripBuilder};
use std::sync::atomic::Ordering::Relaxed;

fn ago(ts: i64) -> String {
    let d = (now_secs() - ts).max(0);
    match d {
        0..=59 => t("az önce", "just now").into(),
        60..=3599 => tf!("{} dk önce", "{} min ago", d / 60),
        3600..=86_399 => tf!("{} saat önce", "{} h ago", d / 3600),
        _ => tf!("{} gün önce", "{} days ago", d / 86_400),
    }
}

/// Sizes of the cleanable (non-red) findings grouped by kind, largest first.
fn distribution(app: &App) -> Vec<(Kind, u64)> {
    let mut by: std::collections::HashMap<Kind, u64> = std::collections::HashMap::new();
    for r in app.rows.iter().filter(|r| r.item.category != Category::Red) {
        *by.entry(r.item.kind()).or_default() += r.item.size;
    }
    let mut v: Vec<_> = by.into_iter().filter(|(_, b)| *b > 0).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

pub fn home(app: &mut App, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            hero(app, ui);
            ui.add_space(14.0);
            quick_actions(app, ui);
            ui.add_space(14.0);
            optimize_card(app, ui);
            ui.add_space(14.0);
            ui.columns(2, |cols| {
                top_files(app, &mut cols[0]);
                distribution_card(app, &mut cols[1]);
            });
            ui.add_space(14.0);
            last_scan(app, ui);
            ui.add_space(14.0);
            unused_card(app, ui);
            bin_card(app, ui);
            ui.add_space(14.0);
            docker_card(app, ui);
        });
}

/// Suggests removing programs that were not opened for a long time (the list comes from the
/// Programs screen; "unknown" last use is never counted as unused).
fn unused_card(app: &mut App, ui: &mut egui::Ui) {
    use draxbyte::core::apps::{self, Removability};
    if !apps::supported() {
        return;
    }
    let ctx = ui.ctx().clone();
    if !app.apps.loaded && app.apps.job.is_none() {
        app.apps_refresh(&ctx);
    }
    if !app.apps.loaded {
        return;
    }
    let (now, days) = (now_secs(), app.apps.unused_days);
    let (n, bytes) = app
        .apps
        .list
        .iter()
        .filter(|a| !a.system_component && !matches!(a.removable, Removability::Sealed(_)))
        .filter(|a| a.days_unused(now).map(|d| d >= days).unwrap_or(false))
        .fold((0usize, 0u64), |(n, b), a| (n + 1, b + a.size_bytes));
    if n == 0 {
        return;
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(t("Kullanılmayan programlar", "Unused programs")).font(super::bold(17.0)));
        ui.label(
            RichText::new(tf!(
                "{} program {} gündür açılmamış · toplam {} yer kaplıyor",
                "{} programs have not been opened for {} days · together they take {}",
                n,
                days,
                human_bytes(bytes)
            ))
            .color(ACCENT),
        );
        ui.label(
            RichText::new(t(
                "Son kullanım Windows/macOS'un kayıtlarından tahmin edilir; kaydı olmayan programlar sayılmaz. Hiçbir şey sen onaylamadan kaldırılmaz.",
                "Last use is estimated from the system's records; programs with no record are not counted. Nothing is removed without your approval.",
            ))
            .color(MUTED)
            .small(),
        );
        if ui.button(t("Gözden geçir ve kaldır…", "Review and remove…")).clicked() {
            app.apps.only_unused = true;
            app.apps.sort = crate::panels::AppSort::Size;
            app.view = View::Programs;
        }
    });
    ui.add_space(14.0);
}

fn bin_card(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if app.bin.status.is_none() && app.bin.job.is_none() && app.bin.error.is_none() {
        app.start_bin(&ctx, false);
    }
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        let name = if cfg!(windows) { t("Geri Dönüşüm Kutusu", "Recycle Bin") } else { t("Çöp Kutusu", "Trash") };
        ui.label(RichText::new(name).font(super::bold(17.0)));
        if let Some(e) = &app.bin.error {
            ui.label(RichText::new(tf!("Okunamadı: {e}", "Could not read: {e}")).color(WARN));
        }
        let busy = app.bin.job.is_some();
        match app.bin.status {
            None => {
                ui.label(RichText::new(t("Okunuyor…", "Reading…")).color(MUTED));
            }
            Some(st) => {
                ui.label(
                    RichText::new(tf!(
                        "{} öğe · {}",
                        "{} items · {}",
                        st.items,
                        human_bytes(st.bytes)
                    ))
                    .color(if st.bytes > 0 { ACCENT } else { MUTED }),
                );
                if let Some(freed) = app.bin.freed.filter(|f| *f != u64::MAX) {
                    ui.label(RichText::new(tf!("{} boşaltıldı.", "{} freed.", human_bytes(freed))).color(ACCENT));
                }
                ui.horizontal(|ui| {
                    if busy {
                        ui.label(RichText::new(t("Çalışıyor…", "Working…")).color(WARN));
                    } else if app.bin.confirm {
                        ui.label(RichText::new(t(
                            "Kutudaki her şey kalıcı olarak silinir ve geri alınamaz. Emin misin?",
                            "Everything in it is deleted for good and cannot be undone. Sure?",
                        )).color(WARN));
                        if ui.button(t("Evet, boşalt", "Yes, empty")).clicked() {
                            app.bin.confirm = false;
                            app.bin.freed = Some(u64::MAX); // marker: next result reports the difference
                            app.start_bin(&ctx, true);
                        }
                        if ui.button(t("Vazgeç", "Cancel")).clicked() {
                            app.bin.confirm = false;
                        }
                    } else {
                        if ui.add_enabled(st.items > 0, egui::Button::new(t("Boşalt", "Empty"))).clicked() {
                            app.bin.confirm = true;
                        }
                        if ui.button(t("Yenile", "Refresh")).clicked() {
                            app.bin.freed = None;
                            app.start_bin(&ctx, false);
                        }
                    }
                });
            }
        }
    });
}

fn hero(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        // StripBuilder fills all available height; pin it to the hero's natural height.
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 205.0), egui::Layout::top_down(egui::Align::Min), |ui| {
        StripBuilder::new(ui).size(Size::remainder()).size(Size::exact(430.0)).horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new(t("Depolama alanınızı geri kazanın.", "Reclaim your storage.")).font(super::bold(30.0)));
                ui.add_space(4.0);
                ui.label(RichText::new(t("Draxbyte diski yerelde tarar, büyük ve gereksiz dosyaları bulur; hiçbir şeyi onayın olmadan silmez. Yapay zekâ ve güncelleme kontrolü kapalıyken hiçbir ağ isteği atılmaz.", "Draxbyte scans your disk locally, finds large and useless files, and never deletes anything without your approval. While AI and the update check are off, no network request is ever made.")).color(MUTED));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let busy = app.scan.is_some() || app.clean_job.is_some();
                    if ui.add_enabled(!busy, primary_button(t("Hızlı tarama", "Quick scan"))).clicked() {
                        let ctx = ui.ctx().clone();
                        app.start_scan(&ctx);
                    }
                    if ui.button(t("Detaylı analiz", "Detailed analysis")).clicked() {
                        app.view = View::Results;
                    }
                });
                ui.add_space(10.0);
                ui.label(RichText::new(t("Taranacak yollar (; ile ayır)", "Paths to scan (separate with ;)")).color(MUTED).small());
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut app.scan_paths_text).desired_width((ui.available_width() - 130.0).max(120.0)));
                    if ui.button(t("Varsayılan", "Default")).clicked() {
                        app.scan_paths_text = app.cfg.scan.default_paths.join("; ");
                    }
                });
                if app.scan_paths_text == app.cfg.scan.default_paths.join("; ") {
                    let extra = match (app.cfg.scan.all_drives, app.cfg.scan.include_system) {
                        (true, true) => t("Ayrıca her dahili sürücü ve sistem klasörleri (Windows Temp, Package Cache…) taranır.", "Every internal drive and the system folders (Windows Temp, Package Cache…) are scanned as well."),
                        (true, false) => t("Ayrıca her dahili sürücü taranır.", "Every internal drive is scanned as well."),
                        (false, true) => t("Ayrıca sistem klasörleri taranır.", "The system folders are scanned as well."),
                        (false, false) => "",
                    };
                    if !extra.is_empty() {
                        ui.label(RichText::new(extra).color(MUTED).small());
                    }
                }
            });
            strip.cell(|ui| {
                ui.horizontal(|ui| {
                    if let Some(d) = &app.disk {
                        let used = d.total.saturating_sub(d.free);
                        let frac = if d.total > 0 { used as f32 / d.total as f32 } else { 0.0 };
                        ring(ui, 190.0, frac, ACCENT, &human_bytes(d.total), &tf!("{} · %{:.0} dolu", "{} · {:.0}% full", d.name, frac * 100.0));
                        ui.add_space(14.0);
                        ui.vertical(|ui| {
                            ui.add_space(52.0);
                            ui.label(RichText::new(t("• Kullanılan", "• Used")).color(ACCENT));
                            ui.label(RichText::new(human_bytes(used)).font(super::bold(17.0)));
                            ui.add_space(6.0);
                            ui.label(RichText::new(t("• Boş alan", "• Free")).color(BLUE));
                            ui.label(RichText::new(human_bytes(d.free)).font(super::bold(17.0)));
                        });
                    }
                });
            });
        });
        });
    });
}

/// Entry to the Full Optimization wizard.
fn optimize_card(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(t("Tam Optimizasyon", "Full Optimization")).font(super::bold(16.0)));
                ui.label(
                    RichText::new(t(
                        "Temizlik, kopya dosyalar, Çöp Kutusu, Docker ve bellek tek akışta. Kapsamı sen seçersin, onayından önce hiçbir şey silinmez.",
                        "Cleanup, duplicates, Recycle Bin, Docker and memory in one flow. You pick the scope; nothing is deleted before you approve.",
                    ))
                    .color(MUTED)
                    .small(),
                );
            });
            if ui.add(primary_button(t("Sihirbazı aç", "Open the wizard"))).clicked() {
                app.view = View::Optimize;
            }
        });
    });
}

fn quick_actions(app: &mut App, ui: &mut egui::Ui) {
    let mut act: Option<u8> = None;
    ui.columns(4, |c| {
        if action_card(
            &mut c[0],
            Icon::Donut,
            t("Disk analizi", "Disk analysis"),
            t(
                "Bulunan her şeyi boyuta göre gör.",
                "See everything found, by size.",
            ),
        ) {
            act = Some(0);
        }
        if action_card(
            &mut c[1],
            Icon::Folder,
            t("Büyük dosyalar", "Large files"),
            t(
                "Ne olduğu belirsiz büyük öğeler.",
                "Big items we can't identify.",
            ),
        ) {
            act = Some(1);
        }
        if action_card(
            &mut c[2],
            Icon::Trash,
            t("Geçici dosyalar", "Temporary files"),
            t("Güvenle silinebilecekler.", "Safe to delete."),
        ) {
            act = Some(2);
        }
        if action_card(
            &mut c[3],
            Icon::Bolt,
            t("Sistem temizliği", "System cleanup"),
            t("Yeşilleri seç ve temizle.", "Select the greens and clean."),
        ) {
            act = Some(3);
        }
    });
    let Some(a) = act else { return };
    if app.rows.is_empty() {
        let ctx = ui.ctx().clone();
        app.start_scan(&ctx);
        return;
    }
    app.filter_text.clear();
    app.view_dirty = true;
    app.view = View::Results;
    app.sort = (SortKey::Size, false);
    match a {
        0 => app.filter_cat = None,
        1 => app.filter_cat = Some(Category::Yellow),
        2 => app.filter_cat = Some(Category::Green),
        _ => {
            app.filter_cat = Some(Category::Green);
            app.select_category(Category::Green);
            if app.selected().next().is_some() {
                app.confirm = Some(Confirm {
                    dry_run: false,
                    permanent: !app.cfg.cleanup.use_trash,
                    typed: String::new(),
                });
            }
        }
    }
}

fn top_files(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(262.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(t(
                    "En büyük temizlenebilir öğeler",
                    "Largest cleanable items",
                ))
                .font(super::bold(16.0)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .link(RichText::new(t("Tümünü gör →", "See all →")).color(ACCENT))
                    .clicked()
                {
                    app.filter_cat = None;
                    app.view_dirty = true;
                    app.view = View::Results;
                }
            });
        });
        ui.add_space(6.0);
        let top: Vec<&crate::app::Row> = app
            .rows
            .iter()
            .filter(|r| r.item.category != Category::Red)
            .take(5)
            .collect();
        if top.is_empty() {
            ui.add_space(30.0);
            ui.label(
                RichText::new(t(
                    "Henüz veri yok. “Hızlı tarama” ile başla.",
                    "No data yet. Start with “Quick scan”.",
                ))
                .color(MUTED),
            );
        }
        for r in top {
            let fname = r
                .item
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| r.item.path.display().to_string());
            // "build", "Cache", "target" say nothing on their own: prefix the parent folder.
            let generic = matches!(
                fname.to_lowercase().as_str(),
                "build" | "cache" | "caches" | "target" | "node_modules" | "temp" | "tmp" | ".cxx"
            );
            let parent_name = r
                .item
                .path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned());
            let name = match (generic, parent_name) {
                (true, Some(pn)) => format!("{pn}{}{fname}", std::path::MAIN_SEPARATOR),
                _ => fname,
            };
            let parent = r
                .item
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            ui.horizontal(|ui| {
                kind_badge(ui, r.item.kind());
                let text_w = (ui.available_width() - 110.0).max(60.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(text_w, 34.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.add(egui::Label::new(RichText::new(name).strong()).truncate());
                        ui.add(
                            egui::Label::new(RichText::new(parent).color(MUTED).small()).truncate(),
                        );
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(human_bytes(r.item.size))
                            .color(category_color(r.item.category)),
                    );
                });
            });
            ui.add_space(2.0);
        }
    });
}

fn distribution_card(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(262.0);
        ui.label(
            RichText::new(t(
                "Temizlenebilir öğelerin dağılımı",
                "Cleanable items by type",
            ))
            .font(super::bold(16.0)),
        );
        ui.label(
            RichText::new(t(
                "Son taramada bulunanlar (kırmızılar hariç)",
                "Found in the last scan (red excluded)",
            ))
            .color(MUTED)
            .small(),
        );
        ui.add_space(8.0);
        let dist = distribution(app);
        let total: u64 = dist.iter().map(|(_, b)| *b).sum();
        if total == 0 {
            ui.add_space(30.0);
            ui.label(RichText::new(t("Henüz veri yok.", "No data yet.")).color(MUTED));
            return;
        }
        let narrow = ui.available_width() < 520.0;
        ui.horizontal(|ui| {
            let segs: Vec<(f32, egui::Color32)> = dist
                .iter()
                .map(|(k, b)| (*b as f32 / total as f32, kind_color(*k)))
                .collect();
            donut(
                ui,
                if narrow { 130.0 } else { 180.0 },
                &segs,
                &human_bytes(total),
                t("toplam", "total"),
            );
            ui.add_space(10.0);
            ui.vertical(|ui| {
                let max = dist.first().map(|(_, b)| *b).unwrap_or(1).max(1) as f32;
                for (k, b) in dist.iter().take(7) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("•").color(kind_color(*k)));
                        ui.add_sized(
                            [if narrow { 112.0 } else { 150.0 }, 18.0],
                            egui::Label::new(k.label()).truncate(),
                        );
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2((ui.available_width() - 120.0).clamp(24.0, 220.0), 8.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 4.0, super::BORDER);
                        let mut fill = rect;
                        fill.set_width((rect.width() * (*b as f32 / max)).max(3.0));
                        ui.painter().rect_filled(fill, 4.0, kind_color(*k));
                        ui.label(
                            RichText::new(tf!(
                                "{}  %{:.0}",
                                "{}  {:.0}%",
                                human_bytes(*b),
                                *b as f32 * 100.0 / total as f32
                            ))
                            .small()
                            .color(MUTED),
                        );
                    });
                }
            });
        });
    });
}

fn last_scan(app: &mut App, ui: &mut egui::Ui) {
    let sum = |c: Category| {
        app.rows
            .iter()
            .filter(|r| r.item.category == c)
            .map(|r| r.item.size)
            .sum::<u64>()
    };
    let count = |c: Category| app.rows.iter().filter(|r| r.item.category == c).count();
    ui.columns(3, |cols| {
        stat_fill(
            &mut cols[0],
            t("Güvenli (yeşil)", "Safe (green)"),
            &tf!(
                "{} · {} öğe",
                "{} · {} items",
                human_bytes(sum(Category::Green)),
                count(Category::Green)
            ),
            ACCENT,
        );
        stat_fill(
            &mut cols[1],
            t("Dikkat (sarı)", "Careful (yellow)"),
            &tf!(
                "{} · {} öğe",
                "{} · {} items",
                human_bytes(sum(Category::Yellow)),
                count(Category::Yellow)
            ),
            WARN,
        );
        stat_fill(
            &mut cols[2],
            t("Silme (kırmızı)", "Keep (red)"),
            &tf!(
                "{} · {} öğe",
                "{} · {} items",
                human_bytes(sum(Category::Red)),
                count(Category::Red)
            ),
            super::DANGER,
        );
    });
    ui.add_space(14.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        match &app.last {
            Some(s) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("Son tarama", "Last scan")).font(super::bold(16.0)));
                    ui.label(RichText::new(ago(s.finished_at)).color(MUTED));
                });
                ui.label(tf!(
                    "{} dosya gezildi · {} öğe ({}) · {} erişilemeyen",
                    "{} files scanned · {} items ({}) · {} inaccessible",
                    s.files_visited,
                    s.item_count,
                    human_bytes(s.total_bytes),
                    s.errors
                ));
                ui.label(RichText::new(s.roots.join("   |   ")).color(MUTED).small());
                if ui.button(t("Sonuçları gör", "View results")).clicked() {
                    app.view = View::Results;
                }
            }
            None => {
                ui.label(t(
                    "Henüz tarama yok. “Hızlı tarama” ile başla.",
                    "No scan yet. Start with “Quick scan”.",
                ));
            }
        }
    });
}

pub fn progress(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        t("Tarama", "Scan"),
        t(
            "Dosyalar yerelde taranıyor; hiçbir şey silinmez.",
            "Files are scanned locally; nothing is deleted.",
        ),
    );
    let Some(job) = &app.scan else {
        ui.label(t("Çalışan tarama yok.", "No scan is running."));
        if ui
            .add(primary_button(t("Ana sayfaya dön", "Back to Home")))
            .clicked()
        {
            app.view = View::Home;
        }
        return;
    };
    let files = job.progress.files.load(Relaxed);
    let bytes = job.progress.bytes.load(Relaxed);
    let found = job.progress.found.load(Relaxed);
    let errors = job.progress.errors.load(Relaxed);
    let current = job
        .progress
        .current
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();
    let secs = job.started.elapsed().as_secs_f32();
    let cancelling = job.cancel.load(Relaxed);

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(28.0).color(ACCENT));
            ui.label(
                RichText::new(if cancelling {
                    t("İptal ediliyor…", "Cancelling…")
                } else {
                    t("Taranıyor…", "Scanning…")
                })
                .size(18.0)
                .strong(),
            );
        });
        ui.add_space(8.0);
        ui.label(tf!(
            "{files} dosya · {} gezildi · {found} aday öğe · {errors} erişilemeyen · {secs:.0} sn", "{files} files · {} scanned · {found} candidate items · {errors} inaccessible · {secs:.0} s",
            human_bytes(bytes)
        ));
        ui.label(RichText::new(current).color(MUTED).small());
        ui.add_space(8.0);
        if ui
            .add_enabled(!cancelling, egui::Button::new(t("İptal", "Cancel")))
            .clicked()
        {
            app.cancel_scan();
        }
    });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}

fn docker_card(app: &mut App, ui: &mut egui::Ui) {
    use draxbyte::core::docker::DockerStatus;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Docker").font(super::bold(17.0)));
            let busy = app.docker_job.is_some();
            let label = if busy {
                t("Kontrol ediliyor…", "Checking…")
            } else {
                t("Docker'ı kontrol et", "Check Docker")
            };
            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                let ctx = ui.ctx().clone();
                app.start_docker_check(&ctx);
            }
        });
        match &app.docker {
            None => {
                ui.label(
                    RichText::new(t(
                        "Docker'ın disk kullanımını (imaj, konteyner, volume, build cache) salt-okunur kontrol eder; hiçbir şeyi silmez.",
                        "Read-only check of Docker's disk usage (images, containers, volumes, build cache); deletes nothing.",
                    ))
                    .color(MUTED),
                );
            }
            Some(DockerStatus::NotInstalled) => {
                ui.label(
                    RichText::new(t(
                        "Docker bulunamadı (PATH'te yok).",
                        "Docker not found (not on PATH).",
                    ))
                    .color(MUTED),
                );
            }
            Some(DockerStatus::DaemonDown(e)) => {
                ui.label(
                    RichText::new(tf!(
                        "Docker çalışmıyor ya da yanıt vermiyor: {e}",
                        "Docker is not running or not responding: {e}"
                    ))
                    .color(WARN),
                );
            }
            Some(DockerStatus::Ok(rows)) => {
                egui::Grid::new("docker_grid")
                    .num_columns(4)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        for h in [
                            t("Tür", "Type"),
                            t("Adet (aktif)", "Count (active)"),
                            t("Boyut", "Size"),
                            t("Geri kazanılabilir", "Reclaimable"),
                        ] {
                            ui.label(RichText::new(h).color(MUTED).small());
                        }
                        ui.end_row();
                        for r in rows {
                            ui.label(&r.kind);
                            ui.label(format!("{} ({})", r.total, r.active));
                            ui.label(&r.size);
                            ui.label(RichText::new(&r.reclaimable).color(ACCENT));
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if app.docker_prune_job.is_some() {
                        ui.label(RichText::new(t("Build cache temizleniyor…", "Cleaning build cache…")).color(WARN));
                    } else if app.docker_confirm_prune {
                        ui.label(RichText::new(t(
                            "Yalnızca build cache silinir (konteyner, imaj, volume'a dokunulmaz). Emin misin?",
                            "Only the build cache is removed (containers, images, volumes untouched). Sure?",
                        )).color(WARN));
                        if ui.button(t("Evet, temizle", "Yes, clean")).clicked() {
                            app.docker_confirm_prune = false;
                            let ctx = ui.ctx().clone();
                            app.start_docker_prune(&ctx);
                        }
                        if ui.button(t("Vazgeç", "Cancel")).clicked() {
                            app.docker_confirm_prune = false;
                        }
                    } else if ui.button(t("Build cache temizle", "Clean build cache")).clicked() {
                        app.docker_confirm_prune = true;
                    }
                });
                ui.label(
                    RichText::new(t(
                        "İmaj/volume temizliği için terminalde docker system prune (volume'larda dikkat — veri içerebilir).",
                        "For images/volumes use docker system prune in a terminal (careful with volumes — they may hold data).",
                    ))
                    .color(MUTED)
                    .small(),
                );
            }
        }
    });
}

/// After a scan: what could not be read, why, and the one-click way to fix it (restart as
/// administrator on Windows, Full Disk Access on macOS). Nothing is left as a bare number.
pub fn errors_banner(app: &mut App, ui: &mut egui::Ui) {
    use draxbyte::core::scanner::ErrKind;
    let Some(e) = app.last_errors.clone() else { return };
    let elevated = draxbyte::core::elevate::is_elevated();
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new(tf!(
                "{} yol okunamadı: {} izin yok · {} başka program kullanıyor · {} diğer",
                "{} paths could not be read: {} access denied · {} in use by another program · {} other",
                e.total(),
                e.permission,
                e.in_use,
                e.other
            ))
            .color(WARN),
        );
        if e.permission > 0 {
            access_buttons(ui, elevated);
        }
        egui::CollapsingHeader::new(RichText::new(t("Hangi yollar?", "Which paths?")).small()).id_salt("scan-errors").show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                for s in &e.samples {
                    let kind = match s.kind {
                        ErrKind::Permission => t("izin yok", "denied"),
                        ErrKind::InUse => t("kullanımda", "in use"),
                        ErrKind::Other => t("diğer", "other"),
                    };
                    ui.add(egui::Label::new(RichText::new(format!("[{kind}] {}", s.path.display())).small().color(MUTED)).truncate());
                }
                if (e.samples.len() as u64) < e.total() {
                    ui.label(RichText::new(tf!("… ilk {} örnek", "… the first {} examples", e.samples.len())).small().color(MUTED));
                }
            });
        });
    });
    ui.add_space(8.0);
}

/// The way out of "access denied": restart as administrator (Windows) or grant Full Disk Access
/// (macOS). Used wherever something could not be read.
pub fn access_buttons(ui: &mut egui::Ui, elevated: bool) {
    ui.horizontal_wrapped(|ui| {
        if cfg!(windows) && !elevated {
            ui.label(RichText::new(t("Bunlar yönetici izni ister.", "These need administrator rights.")).color(MUTED));
            if ui.add(super::widgets::primary_button(t("Yönetici olarak yeniden aç", "Restart as administrator"))).clicked() {
                match draxbyte::core::elevate::relaunch_elevated() {
                    Ok(()) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(m) => {
                        ui.label(RichText::new(tf!("Yönetici izni alınamadı: {m}", "Could not get administrator rights: {m}")).color(WARN));
                    }
                }
            }
        } else if cfg!(target_os = "macos") {
            ui.label(RichText::new(t("macOS bunlar için Tam Disk Erişimi ister.", "macOS needs Full Disk Access for these.")).color(MUTED));
            if ui.add(super::widgets::primary_button(t("Tam Disk Erişimi ayarlarını aç", "Open the Full Disk Access settings"))).clicked() {
                super::widgets::open_full_disk_access();
            }
            ui.label(RichText::new(t("Draxbyte'ı listeye ekleyip uygulamayı yeniden aç.", "Add Draxbyte to the list, then reopen the app.")).color(MUTED).small());
        } else {
            ui.label(RichText::new(t("Yönetici olarak açık olsan da bu yollar sistem tarafından kilitli ya da korumalı.", "Even as administrator the system keeps these locked or protected.")).color(MUTED));
        }
    });
}
