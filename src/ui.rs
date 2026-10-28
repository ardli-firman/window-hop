//! GPUI presentation for Window Hop profile management.

use std::path::PathBuf;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::application::LifecycleCoordinator;
use crate::domain::{DraftProfileInput, ProfileId, ProfileStatus};

/// Tampilan utama pengaturan profil Window Hop.
pub struct WindowHopView {
    coordinator: LifecycleCoordinator,
    selected_profile_id: Option<ProfileId>,
    window_handle: AnyWindowHandle,
    name_input: Entity<InputState>,
    path_input: Entity<InputState>,
    params_input: Entity<InputState>,
    status_message: Option<String>,
    is_error: bool,
}

impl WindowHopView {
    /// Membuat instance baru WindowHopView.
    pub fn new(
        coordinator: LifecycleCoordinator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("misal: Antrian Poli Rawat Jalan"));
        let path_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        });
        let params_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("--app=https://antrian.local --kiosk")
        });

        // Tampilkan pesan error startup langsung jika pembacaan file konfigurasi awal gagal
        let initial_error = coordinator.last_error().map(|s| s.to_string());
        let is_error = initial_error.is_some();

        Self {
            coordinator,
            selected_profile_id: None,
            window_handle: window.window_handle(),
            name_input,
            path_input,
            params_input,
            status_message: initial_error,
            is_error,
        }
    }

    /// Memilih profil untuk diedit atau beralih ke form profil baru.
    pub fn select_profile(
        &mut self,
        id: Option<ProfileId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_profile_id = id.clone();
        self.status_message = None;
        self.is_error = false;

        if let Some(ref pid) = id {
            if let Some(profile) = self.coordinator.get_profile(pid).cloned() {
                let name = profile.name().to_string();
                let path = profile.browser_path().to_string_lossy().to_string();
                let params = profile.custom_parameters().to_string();

                self.name_input.update(cx, |input, cx| {
                    input.set_value(name, window, cx);
                });
                self.path_input.update(cx, |input, cx| {
                    input.set_value(path, window, cx);
                });
                self.params_input.update(cx, |input, cx| {
                    input.set_value(params, window, cx);
                });
            }
        } else {
            // Reset input form untuk profil baru
            self.name_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.path_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.params_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
        }

        cx.notify();
    }

    /// Membuka dialog pemilih file asli OS untuk memilih executable browser secara manual.
    pub fn browse_browser_executable(&mut self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let path_input = self.path_input.clone();

        let prompt_task = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Pilih File Executable Browser".into()),
        });

        cx.spawn(move |_this, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                if let Ok(Ok(Some(paths))) = prompt_task.await {
                    if let Some(path) = paths.into_iter().next() {
                        let path_str = path.to_string_lossy().to_string();
                        cx.update(|cx| {
                            let _ = window_handle.update(cx, |_view, window, cx| {
                                path_input.update(cx, |input, cx| {
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

    /// Menyimpan profil draft secara asinkron tanpa memblokir thread UI.
    /// Operasi disk dijalankan di background executor, dan state memori hanya dikomit
    /// setelah persistensi berhasil.
    pub fn save_profile(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).value().to_string();
        let path_str = self.path_input.read(cx).value().to_string();
        let params = self.params_input.read(cx).value().to_string();

        let browser_path = PathBuf::from(path_str.trim());
        let input = DraftProfileInput::new(name, browser_path, params);

        let prepare_result = self
            .coordinator
            .prepare_draft_save(self.selected_profile_id.as_ref(), &input);

        let (candidate_config, target_id) = match prepare_result {
            Ok(res) => res,
            Err(err) => {
                self.status_message = Some(err.to_string());
                self.is_error = true;
                cx.notify();
                return;
            }
        };

        let store = self.coordinator.store().clone();
        self.status_message = Some("Menyimpan profil draft...".to_string());
        self.is_error = false;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                // Jalankan penulisan disk atomik di background executor
                let to_save = candidate_config.clone();
                let save_result = cx
                    .background_executor()
                    .spawn(async move { store.save(&to_save) })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        match save_result {
                            Ok(()) => {
                                view.coordinator.commit_saved_config(candidate_config);
                                view.selected_profile_id = Some(target_id);
                                view.status_message = Some(
                                    "Profil draft berhasil disimpan secara atomik.".to_string(),
                                );
                                view.is_error = false;
                            }
                            Err(err) => {
                                view.status_message =
                                    Some(format!("Gagal menyimpan ke penyimpanan: {}", err));
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

impl Render for WindowHopView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let profiles = self.coordinator.list_profiles();
        let selected_id = self.selected_profile_id.clone();

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .p_6()
            .gap_4()
            // Header
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Window Hop - Profil Antrian"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Sesi Windows: Tanpa PIN/Akun Tambahan"),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Konfigurasi satu aplikasi browser dan parameter kustom untuk tampilan antrian poliklinik."),
                    ),
            )
            // Banner notifikasi / status error
            .when_some(self.status_message.as_ref(), |this, msg| {
                let bg_color = if self.is_error {
                    theme.danger.opacity(0.15)
                } else {
                    theme.primary.opacity(0.15)
                };
                let text_color = if self.is_error {
                    theme.danger
                } else {
                    theme.primary
                };
                this.child(
                    div()
                        .p_3()
                        .rounded_md()
                        .bg(bg_color)
                        .border_1()
                        .border_color(text_color)
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(div().text_sm().text_color(text_color).child(msg.clone()))
                                .child(
                                    Button::new("btn-dismiss-status")
                                        .label("Tutup")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.status_message = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            })
            // Konten Utama: 2 Kolom
            .child(
                h_flex()
                    .flex_1()
                    .gap_6()
                    // Kolom Kiri: Daftar Profil
                    .child(
                        v_flex()
                            .w_1_3()
                            .gap_3()
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(format!("Daftar Profil ({})", profiles.len())),
                                    )
                                    .child(
                                        Button::new("btn-new-profile")
                                            .label("+ Profil Baru")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.select_profile(None, window, cx);
                                            })),
                                    ),
                            )
                            .child(
                                if profiles.is_empty() {
                                    div()
                                        .p_4()
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .child("Belum ada profil. Klik '+ Profil Baru' untuk membuat.")
                                        .into_any_element()
                                } else {
                                    v_flex()
                                        .gap_2()
                                        .children(profiles.into_iter().map(|p| {
                                            let is_selected = selected_id.as_ref() == Some(p.id());
                                            let pid = p.id().clone();
                                            let bg = if is_selected {
                                                theme.accent.opacity(0.3)
                                            } else {
                                                theme.muted.opacity(0.4)
                                            };
                                            let border = if is_selected {
                                                theme.primary
                                            } else {
                                                theme.border
                                            };
                                            let item_id =
                                                SharedString::from(format!("profile-item-{}", pid));

                                            div()
                                                .id(item_id)
                                                .p_3()
                                                .rounded_md()
                                                .border_1()
                                                .border_color(border)
                                                .bg(bg)
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                    this.select_profile(
                                                        Some(pid.clone()),
                                                        window,
                                                        cx,
                                                    );
                                                }))
                                                .child(
                                                    v_flex()
                                                        .gap_1()
                                                        .child(
                                                            h_flex()
                                                                .justify_between()
                                                                .items_center()
                                                                .child(
                                                                    div()
                                                                        .font_weight(
                                                                            FontWeight::MEDIUM,
                                                                        )
                                                                        .child(p.name().to_string()),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_xs()
                                                                        .px_2()
                                                                        .py_0p5()
                                                                        .rounded_sm()
                                                                        .bg(theme.muted)
                                                                        .text_color(
                                                                            theme.muted_foreground,
                                                                        )
                                                                        .child(match p.status() {
                                                                            ProfileStatus::DraftNotReady => "Draft",
                                                                        }),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(theme.muted_foreground)
                                                                .child(
                                                                    p.browser_path()
                                                                        .to_string_lossy()
                                                                        .to_string(),
                                                                ),
                                                        ),
                                                )
                                        }))
                                        .into_any_element()
                                },
                            ),
                    )
                    // Kolom Kanan: Formulir Detail Profil
                    .child(
                        v_flex()
                            .w_2_3()
                            .gap_4()
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if selected_id.is_some() {
                                        "Edit Profil Draft"
                                    } else {
                                        "Tambah Profil Draft Baru"
                                    }),
                            )
                            // Field Nama Profil
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(div().text_sm().child("Nama Profil:"))
                                    .child(Input::new(&self.name_input)),
                            )
                            // Field Executable Browser + Tombol Browse Manual
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(div().text_sm().child("Jalur Executable Browser:"))
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(Input::new(&self.path_input)),
                                            )
                                            .child(
                                                Button::new("btn-browse-browser")
                                                    .label("Pilih File .exe...")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.browse_browser_executable(cx);
                                                    })),
                                            ),
                                    ),
                            )
                            // Field Parameter Peluncuran Kustom
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(div().text_sm().child("Parameter Peluncuran Kustom:"))
                                    .child(Input::new(&self.params_input)),
                            )
                            // Keterangan Status Kesiapan
                            .child(
                                div()
                                    .p_3()
                                    .rounded_md()
                                    .bg(theme.muted)
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("ℹ Status Kesiapan: Draft (Belum Siap Diaktifkan). Monitor tujuan belum dipilih; aplikasi tidak meluncurkan browser."),
                            )
                            // Tombol Aksi
                            .child(
                                h_flex()
                                    .gap_3()
                                    .child(
                                        Button::new("btn-save-profile")
                                            .primary()
                                            .label("Simpan Profil Draft")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.save_profile(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("btn-reset-form")
                                            .label("Reset")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                let id = this.selected_profile_id.clone();
                                                this.select_profile(id, window, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}
