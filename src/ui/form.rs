//! Profile detail form panel and shortcut generation controls.

use std::path::PathBuf;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::browsers::{BrowserAdapter, BrowserError, LaunchParameters};
use crate::ui::tokens::ShadcnTokens;
use crate::ui::WindowHopView;

/// Memvalidasi apakah parameter baris perintah kustom tingkat lanjut berbenturan
/// dengan isolasi sesi browser profil.
pub fn validate_advanced_parameters(
    advanced: &str,
    adapter: Option<&BrowserAdapter>,
) -> Result<(), BrowserError> {
    LaunchParameters::new("", false, advanced).validate_isolation(adapter)
}

impl WindowHopView {
    /// Renders the profile detail form panel (flex-1).
    pub(crate) fn render_form(
        &mut self,
        shadcn: &ShadcnTokens,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let selected_id = self.selected_profile_id.clone();
        let is_busy = self.is_busy;

        v_flex()
            .flex_1()
            .gap_4()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(shadcn.border)
            .bg(shadcn.card)
            .text_color(shadcn.card_foreground)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if selected_id.is_some() {
                        "Edit Profil"
                    } else {
                        "Tambah Profil Baru"
                    }),
            )
            // Field Nama Profil
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child("Nama Profil:"))
                    .child(Input::new(&self.name_input)),
            )
            // Subkomponen Browser Picker
            .child(self.render_browser_picker(cx))
            // Field URL Antrian Poliklinik
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("URL Antrian Poliklinik:"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Otomatis dijalankan dalam mode windowed app (--app)"),
                            ),
                    )
                    .child(Input::new(&self.url_input)),
            )
            // Sakelar Switch Mode Kiosk Layar Penuh
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .p_3()
                    .rounded_md()
                    .bg(shadcn.muted.opacity(0.15))
                    .border_1()
                    .border_color(shadcn.border)
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Mode Kiosk Layar Penuh (--kiosk)"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Menampilkan jendela antrian secara layar penuh tanpa bilah alamat atau batas jendela"),
                            ),
                    )
                    .child(
                        Switch::new("switch-kiosk")
                            .checked(self.kiosk_mode)
                            .disabled(is_busy)
                            .on_change(cx.listener(|this, checked, _, cx| {
                                this.kiosk_mode = *checked;
                                this.sync_composed_parameters(cx);
                                cx.notify();
                            })),
                    ),
            )
            // Parameter Baris Perintah Kustom Tingkat Lanjut (Collapsible / Accordion)
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .id("toggle-advanced-params")
                            .justify_between()
                            .items_center()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.is_advanced_params_open = !this.is_advanced_params_open;
                                cx.notify();
                            }))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(if self.is_advanced_params_open { "▼" } else { "▶" }),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("Parameter Baris Perintah Kustom Tingkat Lanjut (Opsional)"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(if self.is_advanced_params_open { "Tutup" } else { "Buka Pengaturan Lanjutan" }),
                            ),
                    )
                    .when(self.is_advanced_params_open, |this| {
                        this.child(
                            v_flex()
                                .gap_2()
                                .p_3()
                                .rounded_md()
                                .bg(shadcn.muted.opacity(0.1))
                                .border_1()
                                .border_color(shadcn.border)
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Gunakan untuk flag baris perintah kustom tambahan (misal: --disable-gpu atau --lang=id). Parameter yang berbenturan dengan isolasi profil (seperti --user-data-dir) ditolak secara otomatis."),
                                )
                                .child(Input::new(&self.advanced_params_input))
                                .when_some(self.advanced_params_error.as_ref(), |this, err| {
                                    this.child(
                                        div()
                                            .p_2()
                                            .rounded_md()
                                            .bg(theme.danger.opacity(0.12))
                                            .border_1()
                                            .border_color(theme.danger)
                                            .child(
                                                v_flex()
                                                    .gap_0p5()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.danger)
                                                            .child("⚠ Parameter Isolasi Terlarang Ditolak"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.danger)
                                                            .child(err.clone()),
                                                    ),
                                            ),
                                    )
                                }),
                        )
                    }),
            )
            // Subkomponen Monitor Card
            .child(self.render_monitor_card(cx))
            // Keterangan Status Pergantian / Runtime
            .when_some(self.selected_profile_id.as_ref(), |this, pid| {
                let is_active = self.cached_running_ids.contains(pid);
                if is_active {
                    let has_changes = self.cached_has_running_changes.contains(pid);

                    let mut container = v_flex().gap_2().child(
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(theme.primary.opacity(0.12))
                            .border_1()
                            .border_color(theme.primary)
                            .text_xs()
                            .text_color(theme.primary)
                            .child("● Profil ini sedang aktif berjalan pada monitor tujuan."),
                    );

                    if has_changes {
                        container = container.child(
                            div()
                                .id("notice-unapplied-changes")
                                .p_3()
                                .rounded_md()
                                .bg(theme.warning.opacity(0.12))
                                .border_1()
                                .border_color(theme.warning)
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(theme.warning)
                                                .child("ℹ Perubahan Konfigurasi Belum Diterapkan"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .child("Konfigurasi profil telah diubah dan disimpan, tetapi tayangan antrian saat ini masih menggunakan konfigurasi lama. Klik 'Jalankan Ulang' untuk menerapkan konfigurasi terbaru ke monitor tujuan."),
                                        ),
                                ),
                        );
                    }

                    this.child(container)
                } else {
                    // Periksa apakah ada profil lain yang sedang aktif di monitor tujuan yang sama
                    let conflicting_profile =
                        if let Some(ref target_id) = self.selected_target_monitor {
                            self.cached_profiles.iter().find(|other| {
                                other.id() != pid
                                    && other.target_monitor() == Some(target_id)
                                    && self.cached_running_ids.contains(other.id())
                            })
                        } else {
                            None
                        };

                    if let Some(other) = conflicting_profile {
                        this.child(
                            div()
                                .p_3()
                                .rounded_md()
                                .bg(theme.warning.opacity(0.12))
                                .border_1()
                                .border_color(theme.warning)
                                .text_xs()
                                .text_color(theme.warning)
                                .child(format!(
                                    "ℹ Monitor tujuan saat ini sedang aktif digunakan oleh profil '{}'. Mengaktifkan profil ini akan menutup profil tersebut terlebih dahulu.",
                                    other.name()
                                )),
                        )
                    } else {
                        this
                    }
                }
            })
            // Field Folder Shortcut (Pilihan Folder Alternatif)
            .when(self.selected_profile_id.is_some(), |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Folder Shortcut (Default: Desktop):"),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .child(Input::new(&self.shortcut_folder_input)),
                                )
                                .child(
                                    Button::new("btn-browse-shortcut-folder")
                                        .label("Pilih Folder...")
                                        .disabled(is_busy)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.browse_shortcut_folder(cx);
                                        })),
                                ),
                        ),
                )
            })
            // Dialog Konfirmasi Penghapusan Profil Aktif
            .children(self.render_delete_confirmation_dialog(cx))
            // Pilihan Profil Startup untuk Profil ini
            .when_some(self.selected_profile_id.as_ref(), |this, pid| {
                let is_this_startup = self.cached_startup_profile_id.as_ref() == Some(pid);
                let pid_clone = pid.clone();
                this.child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .p_2p5()
                        .rounded_md()
                        .bg(if is_this_startup {
                            theme.warning.opacity(0.12)
                        } else {
                            shadcn.muted.opacity(0.2)
                        })
                        .border_1()
                        .border_color(if is_this_startup {
                            theme.warning
                        } else {
                            shadcn.border
                        })
                        .child(
                            v_flex()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child("Pengaturan Profil Startup:"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(if is_this_startup {
                                            theme.warning
                                        } else {
                                            shadcn.muted_foreground
                                        })
                                        .child(if is_this_startup {
                                            "★ Profil ini dipilih untuk berjalan otomatis setelah login Windows"
                                        } else {
                                            "Bukan profil startup saat ini"
                                        }),
                                ),
                        )
                        .child(
                            Button::new(if is_this_startup {
                                "btn-unset-startup-profile"
                            } else {
                                "btn-set-startup-profile"
                            })
                            .label(if is_this_startup {
                                "Batalkan Profil Startup"
                            } else {
                                "Jadikan Profil Startup"
                            })
                            .disabled(is_busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if is_this_startup {
                                    this.set_startup_profile(None, cx);
                                } else {
                                    this.set_startup_profile(Some(pid_clone.clone()), cx);
                                }
                            })),
                        ),
                )
            })
            // Tombol Aksi
            .child(self.render_action_bar(cx))
    }

    /// Membuka dialog pemilih folder alternatif untuk penempatan shortcut.
    pub fn browse_shortcut_folder(&mut self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let shortcut_folder_input = self.shortcut_folder_input.clone();

        let prompt_task = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Pilih Folder Alternatif Shortcut".into()),
        });

        cx.spawn(move |_this, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                if let Ok(Ok(Some(paths))) = prompt_task.await {
                    if let Some(path) = paths.into_iter().next() {
                        let path_str = path.to_string_lossy().to_string();
                        cx.update(|cx| {
                            let _ = window_handle.update(cx, |_view, window, cx| {
                                shortcut_folder_input.update(cx, |input, cx| {
                                    input.set_value(path_str, window, cx);
                                });
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }

    /// Membuat shortcut untuk profil yang dipilih (default Desktop atau folder alternatif).
    pub fn create_shortcut_for_selected_profile(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy {
            return;
        }
        let Some(ref pid) = self.selected_profile_id else {
            return;
        };

        let raw_dir = self.shortcut_folder_input.read(cx).value().to_string();
        let custom_dir = if raw_dir.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(raw_dir.trim()))
        };

        self.is_busy = true;
        let pid = pid.clone();
        let coordinator = self.coordinator.clone();
        self.status_message = Some("Membuat shortcut...".to_string());
        self.is_error = false;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let coord_for_task = coordinator.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        coord_for_task
                            .lock()
                            .unwrap()
                            .create_profile_shortcut(&pid, custom_dir)
                    })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.is_busy = false;
                        match result {
                            Ok(shortcut_path) => {
                                view.status_message = Some(format!(
                                    "Shortcut berhasil dibuat di: {}",
                                    shortcut_path.display()
                                ));
                                view.is_error = false;
                            }
                            Err(err) => {
                                view.status_message =
                                    Some(format!("Gagal membuat shortcut: {}", err));
                                view.is_error = true;
                            }
                        }
                        cx.notify();
                    });
                });
            }
        })
        .detach();
    }
}
