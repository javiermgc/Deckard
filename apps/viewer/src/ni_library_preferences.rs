impl crate::App {
    pub(crate) fn native_library_preferences(&mut self, ui: &mut eframe::egui::Ui) {
        ui.strong("Native Access libraries");
        ui.label("Installed NI content is discovered and indexed automatically. Maschine is not required.");
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.refresh_native_libraries(true);
            }
            ui.label(&self.ni_library.status);
        });
        if !self.browser.scan_status.is_empty() {
            ui.label(&self.browser.scan_status);
        }
        if let Some(snapshot) = &self.ni_library.snapshot {
            eframe::egui::ScrollArea::vertical().id_salt("ni-libraries").max_height(160.0).show(ui, |ui| {
                for i in deckard_ni_content::select_installations(&snapshot.report) {
                    ui.label(format!(
                        "{} — {}",
                        i.name,
                        match i.availability {
                            deckard_ni_content::Availability::Available => "Available",
                            deckard_ni_content::Availability::Missing => "Missing",
                            deckard_ni_content::Availability::Unreadable => "Unreadable",
                        }
                    ))
                    .on_hover_text(i.root.map(|p| p.display().to_string()).unwrap_or(i.source));
                }
                for issue in &snapshot.report.issues {
                    ui.label(format!("{}: {}", issue.source, issue.message));
                }
            });
        }
        #[cfg(target_os = "linux")]
        {
            ui.label("Additional Wine prefixes");
            let mut remove = None;
            for (n, path) in self.settings.ni_wine_prefixes.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("Remove").clicked() {
                        remove = Some(n);
                    }
                    ui.label(path.display().to_string());
                });
            }
            let mut changed = false;
            if let Some(n) = remove {
                self.settings.ni_wine_prefixes.remove(n);
                changed = true;
            }
            if ui.button("Add Wine prefix…").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
                && !self.settings.ni_wine_prefixes.contains(&path)
            {
                self.settings.ni_wine_prefixes.push(path);
                changed = true;
            }
            if changed {
                self.settings.save();
                self.refresh_native_libraries(true);
            }
        }
        ui.separator();
        ui.strong("Additional library folders");
    }
}
