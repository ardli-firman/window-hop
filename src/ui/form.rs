//! Wizard step content: application, appearance, monitor, and review.

use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::{Tag as Badge, TagVariant};
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, Disableable as _, Icon, Sizable as _, Theme,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::browsers::LaunchParameters;
use crate::domain::{Profile, ProfileId};
use crate::ui::action_bar::ShortcutOptionsModel;
use crate::ui::profile_wizard::ProfileWizardStep;
use crate::ui::WindowHopView;

/// Model representasi kartu pengaturan profil startup Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupCardModel {
    pub is_startup_profile: bool,
    pub previous_startup_profile_name: Option<String>,
    pub switch_label: &'static str,
    pub description: String,
    pub reassignment_notice: Option<String>,
}

impl StartupCardModel {
    /// Mengevaluasi status kartu startup untuk profil yang sedang ditampilkan.
    pub fn evaluate(
        profile_id: &ProfileId,
        current_startup_id: Option<&ProfileId>,
        profiles: &[Profile],
    ) -> Self {
        let is_startup_profile = current_startup_id == Some(profile_id);

        let previous_startup_profile_name = if !is_startup_profile {
            current_startup_id.and_then(|sid| {
                profiles
                    .iter()
                    .find(|p| p.id() == sid)
                    .map(|p| p.name().to_string())
            })
        } else {
            None
        };

        let description = if is_startup_profile {
            "★ Profil ini dipilih untuk berjalan otomatis setelah login Windows".to_string()
        } else {
            "Bukan profil startup saat ini".to_string()
        };

        let reassignment_notice = previous_startup_profile_name.as_ref().map(|prev_name| {
            format!(
                "Saat ini profil startup diatur ke '{}'. Mengaktifkan sakelar ini akan memindahkan profil startup ke profil ini.",
                prev_name
            )
        });

        Self {
            is_startup_profile,
            previous_startup_profile_name,
            switch_label: "Jalankan otomatis saat Windows login",
            description,
            reassignment_notice,
        }
    }

    /// Menghasilkan pesan informasi keberhasilan pemindahan profil startup.
    pub fn format_transfer_message(from_name: &str, to_name: &str) -> String {
        format!(
            "Profil startup berhasil dipindahkan dari '{}' ke '{}'.",
            from_name, to_name
        )
    }

    /// Menghasilkan pesan informasi pembatalan profil startup.
    pub fn format_unset_message(name: &str) -> String {
        format!(
            "Profil '{}' tidak lagi dijalankan otomatis saat login.",
            name
        )
    }
}

impl WindowHopView {
    /// Langkah 1: identitas profil dan pemilihan browser.
    pub(crate) fn render_wizard_application_step(
        &mut self,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();

        v_flex()
            .w_full()
            .gap_4()
            .child(render_step_intro(&theme, ProfileWizardStep::Application))
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Nama Profil"),
                            )
                            .child(div().text_xs().text_color(theme.danger).child("*")),
                    )
                    .child(Input::new(&self.name_input))
                    .child(
                        div().text_xs().text_color(theme.muted_foreground).child(
                            "Contoh: Antrian Poli Rawat Jalan. Nama ini tampil di dashboard.",
                        ),
                    ),
            )
            .child(self.render_browser_picker(cx))
    }

    /// Langkah 2: URL antrian, mode kiosk, dan parameter lanjutan.
    pub(crate) fn render_wizard_appearance_step(&mut self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_busy = self.is_busy;

        v_flex()
            .w_full()
            .gap_4()
            .child(render_step_intro(
                &theme,
                ProfileWizardStep::Appearance,
            ))
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("URL Antrian Poliklinik"),
                    )
                    .child(Input::new(&self.url_input))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Halaman yang ditampilkan di monitor antrian. Boleh dikosongkan jika browser dibiarkan membuka halaman awal."),
                    ),
            )
            // Sakelar Mode Kiosk Layar Penuh
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .rounded_md()
                    .bg(theme.tokens.group_box)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
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
                                    .child("Tampilkan antrian layar penuh tanpa bilah alamat atau batas jendela."),
                            ),
                    )
                    .child(
                        Switch::new("switch-kiosk")
                            .flex_shrink_0()
                            .checked(self.kiosk_mode)
                            .disabled(is_busy)
                            .on_change(cx.listener(|this, checked, _, cx| {
                                this.kiosk_mode = *checked;
                                cx.notify();
                            })),
                    ),
            )
            // Parameter Baris Perintah Kustom Tingkat Lanjut (Collapsible)
            .child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        h_flex()
                            .id("toggle-advanced-params")
                            .justify_between()
                            .items_center()
                            .flex_wrap()
                            .gap_2()
                            .p_2()
                            .rounded_md()
                            .bg(theme.tokens.group_box)
                            .border_1()
                            .border_color(theme.border)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.is_advanced_params_open = !this.is_advanced_params_open;
                                cx.notify();
                            }))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(if self.is_advanced_params_open {
                                        Icon::new(IconName::ChevronDown)
                                            .size_3p5()
                                            .text_color(theme.muted_foreground)
                                    } else {
                                        Icon::new(IconName::ChevronRight)
                                            .size_3p5()
                                            .text_color(theme.muted_foreground)
                                    })
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("Parameter Baris Perintah Kustom (Opsional)"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(if self.is_advanced_params_open {
                                        "Tutup"
                                    } else {
                                        "Buka Pengaturan Lanjutan"
                                    }),
                            ),
                    )
                    .when(self.is_advanced_params_open, |this| {
                        this.child(
                            v_flex()
                                .gap_2()
                                .p_3()
                                .rounded_md()
                                .bg(theme.tokens.group_box)
                                .border_1()
                                .border_color(theme.border)
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Contoh: --disable-gpu atau --lang=id. Parameter yang berbenturan dengan isolasi profil (seperti --user-data-dir) ditolak otomatis."),
                                )
                                .child(Input::new(&self.advanced_params_input))
                                .when_some(self.advanced_params_error.as_ref(), |this, err| {
                                    this.child(
                                        Alert::error("advanced-params-error", err.clone())
                                            .title("Parameter Isolasi Terlarang Ditolak"),
                                    )
                                }),
                        )
                    }),
            )
            // Pratinjau parameter yang akan diteruskan ke browser
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .bg(theme.muted.opacity(0.4))
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child("Pratinjau parameter peluncuran"),
                    )
                    .child({
                        let adapter = self
                            .cached_browser_compatibility
                            .as_ref()
                            .and_then(|compatibility| compatibility.adapter());
                        let preview = LaunchParameters::new(
                            self.url_input.read(cx).value().to_string(),
                            self.kiosk_mode,
                            self.advanced_params_input.read(cx).value().to_string(),
                        )
                        .compose_for(adapter.as_ref());
                        div()
                            .text_xs()
                            .font_family(theme.mono_font_family.clone())
                            .text_color(theme.foreground)
                            .whitespace_normal()
                            .child(if preview.trim().is_empty() {
                                "(tidak ada parameter tambahan)".to_string()
                            } else {
                                preview
                            })
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Argumen isolasi sesi browser ditambahkan otomatis oleh Window Hop."),
                    ),
            )
    }

    /// Langkah 3: monitor tujuan dan status konflik.
    pub(crate) fn render_wizard_monitor_step(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        v_flex()
            .w_full()
            .gap_4()
            .child(render_step_intro(&theme, ProfileWizardStep::Monitor))
            .child(self.render_monitor_card(cx))
            // Keterangan status profil aktif / konflik monitor
            .when_some(self.selected_profile_id.as_ref(), |this, pid| {
                let is_active = self.cached_running_ids.contains(pid);
                if is_active {
                    let has_changes = self.cached_has_running_changes.contains(pid);
                    let mut container = v_flex().gap_2().child(
                        Alert::info(
                            "notice-profile-active",
                            "Profil ini sedang aktif berjalan pada monitor tujuan.",
                        )
                        .title("Status Profil"),
                    );

                    if has_changes {
                        container = container.child(
                            Alert::warning("notice-unapplied-changes", "Konfigurasi profil telah diubah dan disimpan, tetapi tayangan antrian saat ini masih menggunakan konfigurasi lama. Tutup wizard lalu klik 'Terapkan & Muat Ulang' pada kartu profil untuk menerapkan konfigurasi terbaru.")
                                .title("Perubahan Konfigurasi Belum Diterapkan"),
                        );
                    }

                    this.child(container)
                } else {
                    let conflicting_profile = if let Some(ref target_id) =
                        self.selected_target_monitor
                    {
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
                            Alert::warning(
                                "notice-conflicting-profile",
                                format!(
                                    "Monitor tujuan saat ini sedang aktif digunakan oleh profil '{}'. Mengaktifkan profil ini akan menutup profil tersebut terlebih dahulu.",
                                    other.name()
                                ),
                            )
                            .title("Peringatan Penguasaan Monitor"),
                        )
                    } else {
                        this
                    }
                }
            })
    }

    /// Langkah 4: ringkasan konfigurasi, profil startup, dan pintasan.
    pub(crate) fn render_wizard_review_step(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_editing = self.selected_profile_id.is_some();

        let name = self.name_input.read(cx).value().to_string();
        let browser_path = self.path_input.read(cx).value().to_string();
        let url = self.url_input.read(cx).value().to_string();
        let advanced = self.advanced_params_input.read(cx).value().to_string();
        let monitor_value = self.summary_monitor_label();
        let mode = if self.kiosk_mode {
            "Kiosk layar penuh".to_string()
        } else {
            "Jendela biasa".to_string()
        };

        v_flex()
            .w_full()
            .gap_4()
            .child(render_step_intro(&theme, ProfileWizardStep::Review))
            .child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .bg(theme.tokens.group_box)
                    .border_1()
                    .border_color(theme.border)
                    .child(render_summary_row(
                        &theme,
                        IconName::Settings2,
                        "Nama",
                        if name.trim().is_empty() {
                            "(belum diisi)".to_string()
                        } else {
                            name
                        },
                    ))
                    .child(render_summary_row(
                        &theme,
                        IconName::FileText,
                        "Browser",
                        if browser_path.trim().is_empty() {
                            "(belum dipilih)".to_string()
                        } else {
                            browser_path
                        },
                    ))
                    .child(render_summary_row(
                        &theme,
                        IconName::Globe,
                        "URL",
                        if url.trim().is_empty() {
                            "(halaman awal browser)".to_string()
                        } else {
                            url
                        },
                    ))
                    .child(render_summary_row(
                        &theme,
                        IconName::SquareTerminal,
                        "Monitor",
                        monitor_value,
                    ))
                    .child(render_summary_row(
                        &theme,
                        IconName::Monitor,
                        "Tampilan",
                        mode,
                    ))
                    .when(!advanced.trim().is_empty(), |this| {
                        this.child(render_summary_row(
                            &theme,
                            IconName::Wrench,
                            "Lanjutan",
                            advanced,
                        ))
                    }),
            )
            .when(self.selected_target_monitor.is_none(), |this| {
                this.child(
                    Alert::warning(
                        "notice-draft-profile",
                        "Tanpa monitor tujuan, profil akan disimpan sebagai draft dan belum dapat diaktifkan. Anda dapat memilih monitor kapan saja melalui Edit.",
                    )
                    .title("Profil Akan Disimpan sebagai Draft"),
                )
            })
            .when_some(self.selected_profile_id.clone(), |this, pid| {
                this.child(self.render_startup_card(&pid, cx))
                    .child(self.render_shortcut_section(cx))
            })
            .when(!is_editing, |this| {
                this.child(
                    Alert::info(
                        "notice-after-save",
                        "Setelah profil disimpan, buka kembali melalui tombol Edit untuk memilih profil startup atau membuat pintasan.",
                    )
                    .title("Langkah Setelah Menyimpan"),
                )
            })
    }

    fn summary_monitor_label(&self) -> String {
        match self.selected_target_monitor.as_ref() {
            None => "Belum dipilih (draft)".to_string(),
            Some(identity) => self
                .cached_monitors
                .iter()
                .find(|monitor| &monitor.identity == identity)
                .map(|monitor| format!("{} ({})", monitor.name, identity))
                .unwrap_or_else(|| format!("{} (tidak terhubung)", identity)),
        }
    }

    fn render_startup_card(&self, pid: &ProfileId, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let startup_model = StartupCardModel::evaluate(
            pid,
            self.cached_startup_profile_id.as_ref(),
            &self.cached_profiles,
        );
        let is_this_startup = startup_model.is_startup_profile;
        let pid_clone = pid.clone();

        v_flex()
            .w_full()
            .gap_2()
            .p_3()
            .rounded_md()
            .bg(if is_this_startup {
                theme.warning.opacity(0.12)
            } else {
                *theme.tokens.group_box
            })
            .border_1()
            .border_color(if is_this_startup {
                theme.warning
            } else {
                theme.border
            })
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(IconName::RefreshCw).size_4().text_color(
                                if is_this_startup {
                                    theme.warning
                                } else {
                                    theme.muted_foreground
                                },
                            ))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(startup_model.switch_label),
                            )
                            .when(is_this_startup, |this| {
                                this.child(
                                    Badge::new()
                                        .with_variant(TagVariant::Warning)
                                        .child("★ Profil Startup Aktif"),
                                )
                            }),
                    )
                    .child(
                        Switch::new("switch-startup-profile")
                            .flex_shrink_0()
                            .checked(is_this_startup)
                            .disabled(self.is_busy)
                            .on_change(cx.listener(move |this, checked, _, cx| {
                                if *checked {
                                    this.set_startup_profile(Some(pid_clone.clone()), cx);
                                } else {
                                    this.set_startup_profile(None, cx);
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(if is_this_startup {
                        theme.warning
                    } else {
                        theme.muted_foreground
                    })
                    .child(startup_model.description),
            )
            .child(
                div().text_xs().text_color(theme.muted_foreground).child(
                    "Sakelar ini langsung diterapkan dan tidak menunggu tombol Simpan Profil.",
                ),
            )
            .when_some(
                startup_model.reassignment_notice.as_ref(),
                |this, notice| {
                    this.child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.warning)
                            .child(format!("ℹ {}", notice)),
                    )
                },
            )
    }

    fn render_shortcut_section(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let folder_raw = self.shortcut_folder_input.read(cx).value().to_string();
        let custom_folder = if folder_raw.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(folder_raw.trim()))
        };
        let options = ShortcutOptionsModel::new(custom_folder);

        v_flex()
            .w_full()
            .gap_2()
            .p_3()
            .rounded_md()
            .bg(theme.tokens.group_box)
            .border_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Icon::new(IconName::FileText)
                                    .size_4()
                                    .text_color(theme.muted_foreground),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Pintasan Profil"),
                            ),
                    )
                    .child(
                        Badge::new()
                            .with_variant(TagVariant::Secondary)
                            .child("Default: Desktop"),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Pintasan membuka profil ini tanpa membuka halaman pengaturan."),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .w_full()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.shortcut_folder_input)),
                    )
                    .child(
                        Button::new("btn-browse-shortcut-folder")
                            .flex_shrink_0()
                            .outline()
                            .small()
                            .icon(IconName::FolderOpen)
                            .label("Pilih Folder...")
                            .disabled(self.is_busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.browse_shortcut_folder(cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(format!("Tujuan: {}", options.destination_display())),
                    )
                    .child(
                        Button::new("btn-create-shortcut")
                            .outline()
                            .small()
                            .icon(IconName::ExternalLink)
                            .label("Buat Pintasan")
                            .disabled(self.is_busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_shortcut_for_selected_profile(window, cx);
                            })),
                    ),
            )
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
        if let Some(pid) = self.selected_profile_id.clone() {
            let raw_dir = self.shortcut_folder_input.read(cx).value().to_string();
            let custom_dir = if raw_dir.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(raw_dir.trim()))
            };
            self.create_shortcut(pid, custom_dir, cx);
        }
    }
}

fn render_step_intro(theme: &Theme, step: ProfileWizardStep) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap_1()
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(step.title()),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(step.description()),
        )
}

fn render_summary_row(
    theme: &Theme,
    icon: IconName,
    label: &'static str,
    value: String,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_2()
        .py_1p5()
        .child(
            Icon::new(icon)
                .flex_shrink_0()
                .size_3p5()
                .mt_0p5()
                .text_color(theme.muted_foreground),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(78.0))
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .whitespace_normal()
                .text_color(theme.foreground)
                .child(value),
        )
}

impl WindowHopView {
    /// Membuat pintasan profil pada folder tertentu untuk profil tertentu.
    pub fn create_shortcut(
        &mut self,
        pid: ProfileId,
        custom_dir: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy {
            return;
        }

        self.is_busy = true;
        let coordinator = self.coordinator.clone();
        self.status_message = Some("Membuat pintasan...".to_string());
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
                                view.wizard_error = None;
                                view.status_message = Some(format!(
                                    "Pintasan berhasil dibuat di: {}",
                                    shortcut_path.display()
                                ));
                                view.is_error = false;
                            }
                            Err(err) => {
                                let message = format!("Gagal membuat pintasan: {}", err);
                                view.wizard_error = Some(message.clone());
                                view.status_message = Some(message);
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
