//! Step-by-step modal for creating and editing a Window Hop profile.

use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::browsers::{BrowserAdapter, LaunchParameters};
use crate::ui::WindowHopView;

/// Total number of steps in the profile wizard.
pub const WIZARD_STEP_COUNT: usize = 4;

/// Urutan langkah wizard tambah/edit profil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfileWizardStep {
    #[default]
    Application,
    Appearance,
    Monitor,
    Review,
}

impl ProfileWizardStep {
    /// Semua langkah wizard dalam urutan tampil.
    pub const ALL: [Self; WIZARD_STEP_COUNT] = [
        Self::Application,
        Self::Appearance,
        Self::Monitor,
        Self::Review,
    ];

    /// Indeks numerik langkah (0-based).
    pub fn index(self) -> usize {
        self as usize
    }

    /// Mengambil langkah dari indeks numerik, dibatasi ke rentang yang valid.
    pub fn from_index(index: usize) -> Self {
        Self::ALL[index.min(WIZARD_STEP_COUNT - 1)]
    }

    /// Label singkat untuk stepper.
    pub fn label(self) -> &'static str {
        match self {
            Self::Application => "Aplikasi",
            Self::Appearance => "Tampilan",
            Self::Monitor => "Monitor",
            Self::Review => "Ringkasan",
        }
    }

    /// Judul langkah pada badan modal.
    pub fn title(self) -> &'static str {
        match self {
            Self::Application => "Identitas & Aplikasi",
            Self::Appearance => "Tampilan Antrian",
            Self::Monitor => "Monitor Tujuan",
            Self::Review => "Ringkasan & Pintasan",
        }
    }

    /// Penjelasan singkat tujuan langkah.
    pub fn description(self) -> &'static str {
        match self {
            Self::Application => {
                "Beri nama profil lalu pilih browser yang akan membuka halaman antrian."
            }
            Self::Appearance => {
                "Tentukan halaman antrian dan tampilan jendela yang muncul di monitor."
            }
            Self::Monitor => {
                "Pilih monitor tujuan agar jendela antrian selalu tampil di layar yang benar."
            }
            Self::Review => "Periksa kembali konfigurasi sebelum menyimpan profil antrian.",
        }
    }

    /// Langkah berikutnya bila ada.
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Application => Some(Self::Appearance),
            Self::Appearance => Some(Self::Monitor),
            Self::Monitor => Some(Self::Review),
            Self::Review => None,
        }
    }

    /// Langkah sebelumnya bila ada.
    pub fn previous(self) -> Option<Self> {
        match self {
            Self::Application => None,
            Self::Appearance => Some(Self::Application),
            Self::Monitor => Some(Self::Appearance),
            Self::Review => Some(Self::Monitor),
        }
    }

    /// Apakah langkah ini adalah langkah terakhir.
    pub fn is_last(self) -> bool {
        self == Self::Review
    }
}

/// Kesalahan validasi wizard beserta langkah yang harus diperbaiki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WizardValidationError {
    pub step: ProfileWizardStep,
    pub message: String,
}

impl WizardValidationError {
    fn new(step: ProfileWizardStep, message: impl Into<String>) -> Self {
        Self {
            step,
            message: message.into(),
        }
    }
}

/// Memvalidasi isi satu langkah wizard sebelum pengguna melanjutkan atau menyimpan.
///
/// Aturan ini murni (tanpa GPUI) sehingga dapat diuji terpisah. Langkah
/// `Review` memvalidasi seluruh input yang akan disimpan.
pub fn validate_wizard_step(
    step: ProfileWizardStep,
    name: &str,
    browser_path: &str,
    url: &str,
    advanced_parameters: &str,
    adapter: Option<&BrowserAdapter>,
) -> Result<(), WizardValidationError> {
    match step {
        ProfileWizardStep::Application => {
            if name.trim().is_empty() {
                return Err(WizardValidationError::new(
                    ProfileWizardStep::Application,
                    "Nama profil wajib diisi.",
                ));
            }
            if browser_path.trim().is_empty() {
                return Err(WizardValidationError::new(
                    ProfileWizardStep::Application,
                    "Pilih browser terdeteksi atau isi jalur executable browser.",
                ));
            }
            Ok(())
        }
        ProfileWizardStep::Appearance => validate_appearance(url, advanced_parameters, adapter),
        ProfileWizardStep::Monitor => Ok(()),
        ProfileWizardStep::Review => {
            validate_wizard_step(
                ProfileWizardStep::Application,
                name,
                browser_path,
                url,
                advanced_parameters,
                adapter,
            )?;
            validate_appearance(url, advanced_parameters, adapter)
        }
    }
}

fn validate_appearance(
    url: &str,
    advanced_parameters: &str,
    adapter: Option<&BrowserAdapter>,
) -> Result<(), WizardValidationError> {
    let trimmed_url = url.trim();
    if !trimmed_url.is_empty() {
        let lower = trimmed_url.to_ascii_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return Err(WizardValidationError::new(
                ProfileWizardStep::Appearance,
                "URL antrian harus diawali http:// atau https://.",
            ));
        }
    }

    LaunchParameters::new(trimmed_url, false, advanced_parameters)
        .validate_isolation(adapter)
        .map_err(|error| {
            WizardValidationError::new(
                ProfileWizardStep::Appearance,
                format!("Parameter baris perintah ditolak: {}", error),
            )
        })
}

impl WindowHopView {
    /// Membuka wizard dalam mode tambah profil baru.
    pub fn open_new_profile_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.select_profile(None, window, cx);
        self.is_profile_modal_open = true;
        self.wizard_step = ProfileWizardStep::Application;
        self.wizard_error = None;
        self.wizard_session = self.wizard_session.wrapping_add(1);
        cx.notify();
    }

    /// Membuka wizard dalam mode edit profil yang sudah ada.
    pub fn open_edit_profile_modal(
        &mut self,
        id: crate::domain::ProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_profile(Some(id), window, cx);
        self.is_profile_modal_open = true;
        self.wizard_step = ProfileWizardStep::Application;
        self.wizard_error = None;
        self.wizard_session = self.wizard_session.wrapping_add(1);
        cx.notify();
    }

    /// Menutup wizard tanpa menyimpan perubahan.
    pub fn close_profile_modal(&mut self, cx: &mut Context<Self>) {
        self.is_profile_modal_open = false;
        self.wizard_step = ProfileWizardStep::Application;
        self.wizard_error = None;
        self.selected_profile_id = None;
        self.confirming_delete_profile = None;
        cx.notify();
    }

    /// Pindah ke langkah berikutnya setelah validasi langkah saat ini berhasil.
    pub fn wizard_next(&mut self, cx: &mut Context<Self>) {
        if !self.validate_current_wizard_step(cx) {
            return;
        }
        if let Some(next) = self.wizard_step.next() {
            self.wizard_step = next;
            self.wizard_error = None;
            cx.notify();
        }
    }

    /// Kembali ke langkah sebelumnya.
    pub fn wizard_back(&mut self, cx: &mut Context<Self>) {
        if let Some(previous) = self.wizard_step.previous() {
            self.wizard_step = previous;
            self.wizard_error = None;
            cx.notify();
        }
    }

    /// Pindah ke langkah yang sudah dilewati melalui stepper.
    pub fn wizard_go_to(&mut self, step: ProfileWizardStep, cx: &mut Context<Self>) {
        if step.index() < self.wizard_step.index() {
            self.wizard_step = step;
            self.wizard_error = None;
            cx.notify();
        }
    }

    /// Validasi semua langkah lalu simpan profil.
    pub fn wizard_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy {
            return;
        }
        if !self.validate_all_wizard_steps(cx) {
            return;
        }
        self.save_profile(window, cx);
    }

    fn wizard_adapter(&self) -> Option<BrowserAdapter> {
        self.cached_browser_compatibility
            .as_ref()
            .and_then(|compatibility| compatibility.adapter())
    }

    fn validate_current_wizard_step(&mut self, cx: &mut Context<Self>) -> bool {
        let name = self.name_input.read(cx).value().to_string();
        let browser_path = self.path_input.read(cx).value().to_string();
        let url = self.url_input.read(cx).value().to_string();
        let advanced = self.advanced_params_input.read(cx).value().to_string();
        let adapter = self.wizard_adapter();

        match validate_wizard_step(
            self.wizard_step,
            &name,
            &browser_path,
            &url,
            &advanced,
            adapter.as_ref(),
        ) {
            Ok(()) => {
                self.wizard_error = None;
                true
            }
            Err(error) => {
                self.apply_wizard_error(error);
                cx.notify();
                false
            }
        }
    }

    fn validate_all_wizard_steps(&mut self, cx: &mut Context<Self>) -> bool {
        let name = self.name_input.read(cx).value().to_string();
        let browser_path = self.path_input.read(cx).value().to_string();
        let url = self.url_input.read(cx).value().to_string();
        let advanced = self.advanced_params_input.read(cx).value().to_string();
        let adapter = self.wizard_adapter();

        match validate_wizard_step(
            ProfileWizardStep::Review,
            &name,
            &browser_path,
            &url,
            &advanced,
            adapter.as_ref(),
        ) {
            Ok(()) => {
                self.wizard_error = None;
                true
            }
            Err(error) => {
                self.apply_wizard_error(error);
                cx.notify();
                false
            }
        }
    }

    fn apply_wizard_error(&mut self, error: WizardValidationError) {
        self.wizard_step = error.step;
        if error.step == ProfileWizardStep::Appearance
            && error.message.contains("Parameter baris perintah ditolak")
        {
            self.is_advanced_params_open = true;
            self.advanced_params_error = Some(error.message.clone());
        }
        self.wizard_error = Some(error.message);
    }

    /// Merender modal wizard bila sedang terbuka.
    pub(crate) fn render_profile_wizard(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !self.is_profile_modal_open {
            return None;
        }

        let theme = cx.theme().clone();
        let is_editing = self.selected_profile_id.is_some();
        let profile_name = self
            .selected_profile_id
            .as_ref()
            .and_then(|id| self.find_cached_profile(id))
            .map(|profile| profile.name().to_string());
        let has_unsaved = self
            .selected_profile_id
            .as_ref()
            .map(|id| self.has_unsaved_changes(id, cx))
            .unwrap_or(false);

        let title = if is_editing {
            "Edit Profil"
        } else {
            "Tambah Profil Baru"
        };
        let subtitle = match profile_name.as_deref() {
            Some(name) if is_editing => format!(
                "Perbarui konfigurasi '{}'. Perubahan tidak mengganggu tayangan yang sedang berjalan.",
                name
            ),
            _ => {
                "Ikuti empat langkah singkat berikut untuk menyiapkan jendela antrian."
                    .to_string()
            }
        };

        let wizard_error = self.wizard_error.clone();
        let body = match self.wizard_step {
            ProfileWizardStep::Application => {
                self.render_wizard_application_step(cx).into_any_element()
            }
            ProfileWizardStep::Appearance => {
                self.render_wizard_appearance_step(cx).into_any_element()
            }
            ProfileWizardStep::Monitor => self.render_wizard_monitor_step(cx).into_any_element(),
            ProfileWizardStep::Review => self.render_wizard_review_step(cx).into_any_element(),
        };

        Some(
            div()
                .id("profile-wizard-backdrop")
                .absolute()
                .inset_0()
                .bg(theme.overlay)
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    v_flex()
                        .id("profile-wizard-panel")
                        .w_full()
                        .max_w(px(720.0))
                        .h(px(520.0))
                        .rounded_xl()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.background)
                        .shadow_lg()
                        .overflow_hidden()
                        // Header modal + stepper
                        .child(
                            v_flex()
                                .flex_shrink_0()
                                .gap_3()
                                .p_5()
                                .border_b_1()
                                .border_color(theme.border)
                                .child(
                                    h_flex()
                                        .justify_between()
                                        .items_start()
                                        .gap_3()
                                        .child(
                                            h_flex()
                                                .flex_1()
                                                .min_w_0()
                                                .gap_3()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .p_2()
                                                        .rounded_lg()
                                                        .bg(theme.primary.opacity(0.1))
                                                        .child(
                                                            Icon::new(if is_editing {
                                                                IconName::Settings2
                                                            } else {
                                                                IconName::Plus
                                                            })
                                                            .size_4()
                                                            .text_color(theme.primary),
                                                        ),
                                                )
                                                .child(
                                                    v_flex()
                                                        .min_w_0()
                                                        .gap_1()
                                                        .child(
                                                            h_flex()
                                                                .gap_2()
                                                                .items_center()
                                                                .flex_wrap()
                                                                .child(
                                                                    div()
                                                                        .text_lg()
                                                                        .font_weight(
                                                                            FontWeight::BOLD,
                                                                        )
                                                                        .child(title),
                                                                )
                                                                .when(has_unsaved, |this| {
                                                                    this.child(
                                                                        div()
                                                                            .flex_shrink_0()
                                                                            .text_xs()
                                                                            .font_weight(
                                                                                FontWeight::MEDIUM,
                                                                            )
                                                                            .text_color(
                                                                                theme.warning,
                                                                            )
                                                                            .child(
                                                                                "● Belum disimpan",
                                                                            ),
                                                                    )
                                                                }),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(theme.muted_foreground)
                                                                .child(subtitle),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            Button::new("btn-close-profile-wizard")
                                                .ghost()
                                                .compact()
                                                .label("✕")
                                                .disabled(self.is_busy)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.close_profile_modal(cx);
                                                })),
                                        ),
                                )
                                .child(self.render_wizard_stepper(cx)),
                        )
                        // Isi langkah aktif
                        .child(
                            v_flex()
                                .id(SharedString::from(format!(
                                    "profile-wizard-body-{}-{}",
                                    self.wizard_session,
                                    self.wizard_step.index()
                                )))
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .gap_4()
                                .p_5()
                                .overflow_y_scroll()
                                .when_some(wizard_error, |this, message| {
                                    this.child(
                                        Alert::error("wizard-error", message)
                                            .title("Periksa Langkah Ini"),
                                    )
                                })
                                .child(body),
                        )
                        // Footer navigasi
                        .child(self.render_wizard_footer(is_editing, cx)),
                ),
        )
    }

    fn render_wizard_stepper(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current = self.wizard_step;

        let mut children: Vec<AnyElement> = Vec::new();
        for step in ProfileWizardStep::ALL {
            let is_current = step == current;
            let is_completed = step.index() < current.index();
            let label_color = if is_current {
                theme.foreground
            } else if is_completed {
                theme.primary
            } else {
                theme.muted_foreground
            };

            children.push(
                h_flex()
                    .id(SharedString::from(format!("wizard-step-{}", step.index())))
                    .flex_shrink_0()
                    .gap_2()
                    .items_center()
                    .when(is_completed, |this| {
                        this.cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.wizard_go_to(step, cx);
                            }))
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if is_completed {
                                theme.primary.opacity(0.14)
                            } else if is_current {
                                theme.primary
                            } else {
                                theme.muted
                            })
                            .when(is_completed, |this| {
                                this.child(
                                    Icon::new(IconName::Check)
                                        .size_3()
                                        .text_color(theme.primary),
                                )
                            })
                            .when(!is_completed, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(if is_current {
                                            theme.background
                                        } else {
                                            theme.muted_foreground
                                        })
                                        .child(format!("{}", step.index() + 1)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(if is_current {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .text_color(label_color)
                            .child(step.label()),
                    )
                    .into_any_element(),
            );

            if !step.is_last() {
                children.push(
                    div()
                        .flex_1()
                        .h(px(1.0))
                        .min_w(px(12.0))
                        .mx_2()
                        .bg(if is_completed {
                            theme.primary.opacity(0.35)
                        } else {
                            theme.border
                        })
                        .into_any_element(),
                );
            }
        }

        h_flex().w_full().items_center().children(children)
    }

    fn render_wizard_footer(&self, is_editing: bool, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let step = self.wizard_step;
        let is_busy = self.is_busy;

        h_flex()
            .flex_shrink_0()
            .justify_between()
            .items_center()
            .flex_wrap()
            .gap_3()
            .p_4()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.tokens.group_box)
            .child(h_flex().gap_2().items_center().when(is_editing, |this| {
                this.child(
                    Button::new("btn-wizard-delete")
                        .ghost()
                        .danger()
                        .small()
                        .icon(IconName::Delete)
                        .label("Hapus Profil")
                        .disabled(is_busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.request_delete_selected_profile(cx);
                        })),
                )
            }))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "Langkah {} dari {}",
                                step.index() + 1,
                                WIZARD_STEP_COUNT
                            )),
                    )
                    .child(
                        Button::new("btn-wizard-cancel")
                            .outline()
                            .label("Batal")
                            .disabled(is_busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_profile_modal(cx);
                            })),
                    )
                    .when(step.previous().is_some(), |this| {
                        this.child(
                            Button::new("btn-wizard-back")
                                .ghost()
                                .icon(IconName::ChevronLeft)
                                .label("Kembali")
                                .disabled(is_busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.wizard_back(cx);
                                })),
                        )
                    })
                    .child(if step.is_last() {
                        Button::new("btn-wizard-save")
                            .primary()
                            .icon(IconName::Check)
                            .label(if is_busy {
                                "Menyimpan..."
                            } else {
                                "Simpan Profil"
                            })
                            .disabled(is_busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.wizard_save(window, cx);
                            }))
                            .into_any_element()
                    } else {
                        Button::new("btn-wizard-next")
                            .primary()
                            .icon(IconName::ChevronRight)
                            .label("Lanjut")
                            .disabled(is_busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.wizard_next(cx);
                            }))
                            .into_any_element()
                    }),
            )
    }
}

impl WindowHopView {
    /// Membuka wizard edit langsung pada langkah tertentu (misal Monitor untuk profil draft).
    pub fn open_edit_profile_modal_at(
        &mut self,
        id: crate::domain::ProfileId,
        step: ProfileWizardStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_edit_profile_modal(id, window, cx);
        self.wizard_step = step;
        cx.notify();
    }
}
