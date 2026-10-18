//! Duplicate finder screen: groups of identical files, which copy to keep, and a confirmed delete.

use super::widgets::{card, danger_button, heading, primary_button, reveal};
use super::{ACCENT, MUTED, WARN};
use crate::app::App;
use crate::map_dupes::MAX_GROUPS;
use draxbyte::core::dupes::{Keep, STAGE_FULL, STAGE_INDEX, STAGE_QUICK};
use draxbyte::i18n::t;
use draxbyte::models::{human_bytes, now_secs};
use draxbyte::tf;
use egui::RichText;
use std::sync::atomic::Ordering::Relaxed;

pub fn duplicates(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        t("Kopya Dosyalar", "Duplicate Files"),
        t(
            "Birebir aynı içerikli dosyaları bulur (boyut → ilk/son 64 KB → tam SHA-256). Her gruptan en az bir kopya her zaman kalır.",
            "Finds files with exactly the same content (size → first/last 64 KB → full SHA-256). At least one copy of every group always stays.",
        ),
    );
    let ctx = ui.ctx().clone();

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t("Yollar (; ile ayır):", "Paths (separate with ;):")).color(MUTED));
            ui.add(egui::TextEdit::singleline(&mut app.dupes.roots_text).desired_width(320.0));
            ui.label(RichText::new(t("En az", "At least")).color(MUTED));
            ui.add(egui::DragValue::new(&mut app.dupes.min_mb).range(0..=100_000).suffix(" MB"));
            ui.checkbox(&mut app.dupes.skip_dev, t("Geliştirme klasörlerini atla", "Skip developer folders"))
                .on_hover_text(t("node_modules, .git, target, __pycache__… içindeki kopyalar bilerek vardır", "Copies inside node_modules, .git, target, __pycache__… are there by design"));
            if let Some(j) = &app.dupes.job {
                if ui.button(t("İptal", "Cancel")).clicked() {
                    j.cancel.store(true, Relaxed);
                }
            } else if ui.add(primary_button(t("Kopyaları ara", "Find duplicates"))).clicked() {
                app.dupes_start(&ctx);
            }
        });
        if let Some(j) = &app.dupes.job {
            let p = &j.progress;
            let line = match p.stage.load(Relaxed) {
                s if s == STAGE_INDEX => tf!("Dosyalar taranıyor… {} dosya", "Indexing files… {} files", p.files_seen.load(Relaxed)),
                s if s == STAGE_QUICK => tf!("Aday {} dosya hızlı kontrol ediliyor…", "Quick-checking {} candidate files…", p.candidates.load(Relaxed)),
                s if s == STAGE_FULL => tf!("Tam karşılaştırma: {} / {}", "Full comparison: {} / {}", p.hashed.load(Relaxed), p.to_hash.load(Relaxed)),
                _ => String::new(),
            };
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(format!("{line} · {} sn", j.started.elapsed().as_secs())).color(MUTED));
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    });
    ui.add_space(8.0);

    if app.dupes.job.is_some() {
        return;
    }
    if !app.dupes.searched {
        ui.label(RichText::new(t("\"Kopyaları ara\"ya basınca seçilen yollar taranır. Hiçbir şey silinmez; silmeden önce her şeyi sen seçersin.", "Press \"Find duplicates\" to scan the paths. Nothing is deleted; you choose everything before any deletion.")).color(MUTED));
        return;
    }
    if app.dupes.groups.is_empty() {
        ui.label(RichText::new(t("Kopya dosya bulunamadı.", "No duplicate files found.")).color(ACCENT));
        return;
    }

    let total = app.dupes.groups.len();
    ui.label(
        RichText::new(tf!(
            "{} kopya grubu · boşa giden alan: {}",
            "{} groups of duplicates · wasted space: {}",
            total,
            human_bytes(app.dupes.wasted())
        ))
        .font(super::bold(16.0))
        .color(ACCENT),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(t("Otomatik seç:", "Auto-select:")).color(MUTED));
        for (label, rule) in [
            (t("En eskiyi koru", "Keep the oldest"), Keep::Oldest),
            (t("En yeniyi koru", "Keep the newest"), Keep::Newest),
            (t("En kısa yolu koru", "Keep the shortest path"), Keep::ShortestPath),
        ] {
            if ui.selectable_label(app.dupes.rule == rule, label).clicked() {
                app.dupes_apply_rule(rule);
            }
        }
    });
    if total > MAX_GROUPS {
        ui.label(RichText::new(tf!("En çok yer harcayan ilk {} grup gösteriliyor; silme yalnızca görünenlere uygulanır.", "Showing the {} groups that waste the most; deletion applies only to what you see.", MAX_GROUPS)).color(WARN).small());
    }
    ui.add_space(4.0);

    let now = now_secs();
    let mut toggles: Vec<(usize, usize, bool)> = Vec::new();
    egui::ScrollArea::vertical().max_height((ui.available_height() - 74.0).max(160.0)).show(ui, |ui| {
        for (gi, g) in app.dupes.groups.iter().enumerate().take(MAX_GROUPS) {
            let first = g.group.files[0].path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let title = format!(
                "{first}  ·  {}  ·  {}",
                tf!("{} kopya × {}", "{} copies × {}", g.group.files.len(), human_bytes(g.group.size)),
                tf!("{} boşa", "{} wasted", human_bytes(g.group.wasted()))
            );
            egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(("dupe-group", gi)).default_open(false).show(ui, |ui| {
                for (fi, f) in g.group.files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let mut del = g.delete[fi];
                        if ui.checkbox(&mut del, "").changed() {
                            toggles.push((gi, fi, del));
                        }
                        let days = ((now - f.mtime).max(0)) / 86_400;
                        let kept = !g.delete[fi];
                        ui.add_sized([70.0, 18.0], egui::Label::new(RichText::new(if kept { t("KALIR", "KEPT") } else { t("silinecek", "to delete") }).color(if kept { ACCENT } else { MUTED }).small()));
                        ui.add_sized([54.0, 18.0], egui::Label::new(RichText::new(tf!("{} g", "{} d", days)).color(MUTED).small()));
                        ui.add(egui::Label::new(f.path.display().to_string()).truncate());
                        if ui.small_button(t("Göster", "Show")).clicked() {
                            reveal(&f.path);
                        }
                    });
                }
            });
        }
    });
    for (g, f, on) in toggles {
        if !app.dupes.groups[g].set(f, on) {
            app.toast(t("Her gruptan en az bir kopya kalmalı", "At least one copy of every group must stay"));
        }
    }

    // ---- delete bar
    ui.add_space(6.0);
    let (n, bytes) = app.dupes.selected_count_bytes();
    ui.horizontal_wrapped(|ui| {
        if app.dupes.clean_job.is_some() {
            ui.spinner();
            ui.label(t("Siliniyor…", "Deleting…"));
        } else if app.dupes.confirm {
            ui.label(RichText::new(tf!("{} dosya ({}) {}. Her gruptan bir kopya kalır. Emin misin?", "{} files ({}) will be {}. One copy of every group stays. Sure?", n, human_bytes(bytes), if app.dupes.permanent { t("kalıcı silinecek", "deleted for good") } else { t("Çöp Kutusu'na taşınacak", "moved to the Recycle Bin") })).color(WARN));
            if ui.add(danger_button(t("Evet", "Yes"))).clicked() {
                app.dupes_delete(&ctx);
            }
            if ui.button(t("Vazgeç", "Cancel")).clicked() {
                app.dupes.confirm = false;
            }
        } else {
            if ui.add_enabled(n > 0, primary_button(&tf!("Seçilenleri kaldır ({}, {})", "Remove selected ({}, {})", n, human_bytes(bytes)))).clicked() {
                app.dupes.confirm = true;
            }
            ui.checkbox(&mut app.dupes.permanent, t("Kalıcı sil (Çöp Kutusu'nu atla)", "Delete for good (skip the Recycle Bin)"));
        }
    });
}
