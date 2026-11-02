//! The owner gate in the window (`horizon_launcher::account::launch`): the screen that asks for a
//! Microsoft account that owns Minecraft: Java Edition. It covers the launcher once for players
//! without one (those who played offline before the gate), and again whenever Play is refused.
//! The sign-in runs on it, code and all.
use super::*;

impl Launcher {
    /// The first-run question: Microsoft or Ely.by. It comes before the Microsoft owner gate, so
    /// an Ely.by player never has to sign in to Microsoft first.
    pub(crate) fn asking_provider(&self) -> bool {
        self.provider_open
            || page_preview("choose")
            || (!self.accounts.provider_chosen
                && self.accounts.accounts.is_empty()
                && self.accounts.elyby.is_empty()
                && !self.owner_screen_open
                && !self.ely_screen_open
                && !self.settings.owner_notice_seen
                && !self.asking_consent()
                && self.opening.is_none())
    }

    /// Shows the ways to play again, from Settings or the top bar.
    pub(crate) fn open_provider(&mut self) {
        self.cancel_sign_in();
        self.owner_screen_open = false;
        self.ely_screen_open = false;
        self.provider_open = true;
    }

    /// Plays offline: no account, no Microsoft owner gate, the game's own name.
    pub(crate) fn choose_offline(&mut self) {
        let mut accounts = Accounts::load();
        let done = accounts
            .play_as(account::Player::Offline)
            .and_then(|()| accounts.save());
        match done {
            Ok(()) => {
                self.accounts = accounts;
                self.provider_open = false;
                self.owner_screen_open = false;
                self.ely_screen_open = false;
                self.settings.owner_notice_seen = true;
                let _ = self.settings.save();
            }
            Err(e) => self.client_error = Some(e),
        }
    }

    pub(crate) fn provider_screen(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        if crate::glass_ui::on() {
            return self.glass_provider_screen(cx);
        }
        let t = self.theme;
        let n = t.night;
        let card = plate::cut(
            theme::night::PIXEL_CORNER,
            2,
            Some(plate::alpha(n.card, 0.98)),
            Some((plate::alpha(n.stroke, 2. * n.stroke_alpha), 1.)),
        )
        .flex()
        .flex_col()
        .gap(px(12.))
        .w(px(560.))
        .max_w_full()
        .p(px(32.))
        .child(
            shell::text(theme::night::CAPTION, shell::c(n.muted))
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr("Account").to_string()),
        )
        .child(
            shell::text(theme::night::TITLE, shell::c(n.ink))
                .mb(px(4.))
                .child(tr("Choose how to play").to_string()),
        )
        .child(
            shell::text(theme::night::UI, shell::c(n.ink_soft)).child(
                tr("Use a Microsoft account or an Ely.by account. You can change this later in Settings.")
                    .to_string(),
            ),
        )
        .child(
            button("provider-microsoft", "Microsoft account", Kind::Primary, &t)
                .w_full()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.provider_open = false;
                    this.owner_screen_open = true;
                    this.start_sign_in();
                    cx.notify();
                })),
        )
        .child(
            button("provider-offline", "Play offline", Kind::Secondary, &t)
                .w_full()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.choose_offline();
                    cx.notify();
                })),
        )
        .child(
            button("provider-elyby", "Ely.by account", Kind::Secondary, &t)
                .w_full()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.provider_open = false;
                    this.ely_screen_open = true;
                    this.ely_state = None;
                    cx.notify();
                })),
        )
        .child(div().flex().justify_end().mt(px(6.)).child(
            shell::action("provider-later", shell::Action::Quiet, &t)
                .child(shell::label(tr("Not now").to_string()))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.provider_open = false;
                    this.settings.owner_notice_seen = true;
                    let _ = this.settings.save();
                    cx.notify();
                })),
        ));
        self.gate("provider-gate", card)
    }

    pub(crate) fn ely_screen(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        if crate::glass_ui::on() {
            return self.glass_ely_screen(cx);
        }
        let t = self.theme;
        let n = t.night;
        let login = self
            .ely_field(
                "elyby-login",
                &self.ely_login,
                self.ely_login_focused,
                &self.ely_login_focus,
                "Ely.by login or e-mail",
                false,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                window.focus(&this.ely_login_focus, cx);
                this.ely_login_focused = true;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match line_edit::handle(&mut this.ely_login, ev, cx) {
                    line_edit::Key::Pass => return,
                    line_edit::Key::Escape => window.blur(cx),
                    _ => {}
                }
                cx.stop_propagation();
                cx.notify();
            }));
        let password = self
            .ely_field(
                "elyby-password",
                &self.ely_password,
                self.ely_password_focused,
                &self.ely_password_focus,
                "Password",
                true,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                window.focus(&this.ely_password_focus, cx);
                this.ely_password_focused = true;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match line_edit::handle(&mut this.ely_password, ev, cx) {
                    line_edit::Key::Pass => return,
                    line_edit::Key::Escape => window.blur(cx),
                    _ => {}
                }
                cx.stop_propagation();
                cx.notify();
            }));
        let two_factor = self.ely_state == Some(ElySignInState::TwoFactor);
        let code = two_factor.then(|| {
            self.ely_field(
                "elyby-code",
                &self.ely_code,
                self.ely_code_focused,
                &self.ely_code_focus,
                "Two-factor code",
                false,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                window.focus(&this.ely_code_focus, cx);
                this.ely_code_focused = true;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match line_edit::handle(&mut this.ely_code, ev, cx) {
                    line_edit::Key::Pass => return,
                    line_edit::Key::Escape => window.blur(cx),
                    _ => {}
                }
                cx.stop_propagation();
                cx.notify();
            }))
        });
        let state = match &self.ely_state {
            None => String::new(),
            Some(ElySignInState::Starting) => tr("Signing in to Ely.by…").to_string(),
            Some(ElySignInState::TwoFactor) => {
                tr("Enter the two-factor code from your authenticator.").to_string()
            }
            Some(ElySignInState::Done(Ok(account))) => {
                trf("Signed in as {name}.", &[("name", &account.name)])
            }
            Some(ElySignInState::Done(Err(e))) => e.clone(),
        };
        let busy = matches!(self.ely_state, Some(ElySignInState::Starting));
        let sign_in = button(
            "elyby-sign-in",
            if busy { "Signing in…" } else { "Sign in" },
            Kind::Primary,
            &t,
        );
        let sign_in = if busy {
            disabled(sign_in)
        } else {
            sign_in.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.ely_sign_in_pressed();
                cx.notify();
            }))
        };
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                shell::text(theme::night::CAPTION, shell::c(n.muted))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("Ely.by account").to_string()),
            )
            .child(
                shell::text(theme::night::TITLE, shell::c(n.ink))
                    .mb(px(4.))
                    .child(tr("Sign in to play").to_string()),
            )
            .child(
                shell::text(theme::night::UI, shell::c(n.ink_soft)).child(
                    tr("Your password goes only to authserver.ely.by. The launcher keeps the token, never the password.")
                        .to_string(),
                ),
            )
            .child(login)
            .child(password);
        if let Some(code) = code {
            body = body.child(code);
        }
        if !state.is_empty() {
            body = body.child(
                shell::text(
                    theme::night::UI,
                    shell::c(if two_factor { n.ink_soft } else { n.muted }),
                )
                .child(state),
            );
        }
        let card = plate::cut(
            theme::night::PIXEL_CORNER,
            2,
            Some(plate::alpha(n.card, 0.98)),
            Some((plate::alpha(n.stroke, 2. * n.stroke_alpha), 1.)),
        )
        .flex()
        .flex_col()
        .gap(px(8.))
        .w(px(520.))
        .max_w_full()
        .p(px(32.))
        .child(body)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .mt(px(14.))
                .child(sign_in)
                .child(
                    shell::action("elyby-back", shell::Action::Quiet, &t)
                        .child(shell::label(tr("Cancel").to_string()))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.ely_cancel();
                            cx.notify();
                        })),
                ),
        );
        self.gate("ely-gate", card)
    }

    /// A sheet over the whole window: the launcher dimmed behind a card that takes its scroll.
    fn gate(&self, id: &'static str, card: Div) -> Stateful<Div> {
        let n = self.theme.night;
        div()
            .id(id)
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px(px(32.))
            .py(px(24.))
            .bg(plate::alpha(n.page, 0.8))
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .occlude()
            .child(card)
    }

    fn ely_field(
        &self,
        id: &'static str,
        edit: &line_edit::LineEdit,
        focused: bool,
        focus: &FocusHandle,
        placeholder: &'static str,
        mask: bool,
    ) -> Stateful<Div> {
        let t = self.theme;
        div()
            .id(id)
            .track_focus(focus)
            .flex()
            .items_center()
            .w_full()
            .h(px(34.))
            .px(px(12.))
            .overflow_hidden()
            .border_2()
            .border_color(if focused { t.accent } else { t.outline })
            .bg(t.well)
            .font_family(theme::MONO)
            .text_size(px(13.))
            .cursor_text()
            .child(line_edit::content(
                edit,
                &t,
                focused,
                false,
                mask,
                placeholder,
            ))
    }
    /// Whether the screen covers the window: no owner, and either Play was refused or the player
    /// hasn't read it yet. The data question comes first.
    pub(crate) fn asking_owner(&self) -> bool {
        (self.owner == Some(false) || sign_in_preview())
            && (sign_in_preview()
                || !matches!(
                    self.accounts.player(),
                    Some(account::Player::ElyBy(_) | account::Player::Offline)
                ))
            && (self.owner_screen_open || !self.settings.owner_notice_seen || sign_in_preview())
            && !self.asking_provider()
            && !self.ely_screen_open
            && !page_preview("elyby")
            && !self.asking_consent()
            && self.opening.is_none()
    }

    /// Opens the screen and starts the sign-in on it.
    pub(crate) fn sign_in_to_play(&mut self) {
        self.owner_screen_open = true;
        self.start_sign_in();
    }

    /// Closes it; it comes back with the next refused Play.
    fn close_owner_screen(&mut self) {
        self.cancel_sign_in();
        self.owner_screen_open = false;
        if !self.settings.owner_notice_seen {
            self.settings.owner_notice_seen = true;
            self.save_settings();
        }
    }

    pub(crate) fn owner_screen(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let n = t.night;
        let configured = account::client_id().is_some();
        // The screenshot preview shows a code without asking Microsoft for one.
        let state = match &self.sign_in_state {
            _ if sign_in_preview() => Shown::Code("https://www.microsoft.com/link", "K7Q-4MXR"),
            _ if !configured => Shown::Error(tr(account::NOT_CONFIGURED)),
            Some(SignInState::Starting) => Shown::Starting("Asking Microsoft…"),
            Some(SignInState::Working(phase)) => Shown::Starting(phase.label()),
            Some(SignInState::Code(code)) => Shown::Code(&code.verification_uri, &code.user_code),
            Some(SignInState::Done(Err(e))) => Shown::Error(e),
            Some(SignInState::Done(Ok(_))) | None => Shown::Idle,
        };
        if crate::glass_ui::on() {
            return self.glass_owner_screen(state, configured, cx);
        }
        let line = |s: &str, color: u32| {
            shell::text(theme::night::UI, shell::c(color))
                .relative()
                .child(tr(s).to_string())
        };
        let mut body = div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                shell::text(theme::night::CAPTION, shell::c(n.muted))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("Microsoft account").to_string()),
            )
            .child(
                shell::text(theme::night::TITLE, shell::c(n.ink))
                    .mb(px(4.))
                    .child(tr("Sign in to play").to_string()),
            )
            .child(line(
                "Horizon needs a Microsoft account that owns Minecraft: Java Edition, like other launchers.",
                n.ink_soft,
            ))
            .child(line(
                "No Minecraft install needed: Horizon downloads it from Mojang for your account.",
                n.ink_soft,
            ));
        let raised = || {
            div()
                .relative()
                .mt(px(10.))
                .p(px(18.))
                .rounded(px(theme::night::RADIUS_CONTROL))
                .bg(plate::alpha(n.card_hi, n.card_hi_alpha))
                .border_1()
                .border_color(plate::alpha(n.stroke, n.stroke_alpha))
        };
        body = match state {
            Shown::Code(uri, user_code) => {
                let copy = user_code.to_string();
                let (private_uri, link_uri, other_uri) =
                    (uri.to_string(), uri.to_string(), uri.to_string());
                body.child(
                    raised()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(
                                    shell::text(theme::night::CAPTION, shell::c(n.ink_soft)).child(
                                        trf(
                                            "Open {link} in your browser and enter this code:",
                                            // The page as the player would type it.
                                            &[("link", short_link(uri))],
                                        ),
                                    ),
                                )
                                .child(
                                    div()
                                        .font_family(theme::MONO)
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(30.))
                                        .line_height(px(38.))
                                        .text_color(shell::c(n.ink))
                                        .child(user_code.to_string()),
                                ),
                        )
                        .child(
                            shell::action("owner-copy", shell::Action::Raised, &t)
                                .child(shell::label(tr("Copy code").to_string()))
                                .on_click(move |_, _, cx| {
                                    copy_to_clipboard(
                                        cx,
                                        gpui::ClipboardItem::new_string(copy.clone()),
                                    )
                                }),
                        ),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(4.))
                        .when(!microsoft::has_private_window(), |col| {
                            col.child(shell::text(theme::night::CAPTION, shell::c(n.muted)).child(
                                tr("Open this link in a private window to pick another account."),
                            ))
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(if microsoft::has_private_window() {
                                    shell::action("owner-private", shell::Action::Quiet, &t)
                                        .child(shell::label(tr("Open in private window")))
                                        .on_click(move |_, _, cx| {
                                            microsoft::open_private(&private_uri, cx)
                                        })
                                } else {
                                    shell::action("owner-copy-link", shell::Action::Quiet, &t)
                                        .child(shell::label(tr("Copy link")))
                                        .on_click(move |_, _, cx| {
                                            copy_to_clipboard(
                                                cx,
                                                gpui::ClipboardItem::new_string(link_uri.clone()),
                                            )
                                        })
                                })
                                .child(
                                    shell::action("owner-other", shell::Action::Quiet, &t)
                                        .child(shell::label(tr(
                                            "Use a different Microsoft account",
                                        )))
                                        .on_click(move |_, _, cx| {
                                            microsoft::use_different_account(&other_uri, cx)
                                        }),
                                ),
                        ),
                )
            }
            Shown::Starting(what) => body.child(raised().child(
                shell::text(theme::night::UI, shell::c(n.ink_soft)).child(tr(what).to_string()),
            )),
            Shown::Error(e) => body.child(
                raised()
                    .child(shell::text(theme::night::UI, shell::c(n.danger)).child(e.to_string())),
            ),
            Shown::Idle => body,
        };
        let main = match state {
            Shown::Code(uri, _) => {
                let uri = uri.to_string();
                Some(
                    shell::action("owner-open", shell::Action::Main, &t)
                        .child(shell::label(tr("Open link").to_string()))
                        .on_click(move |_, _, cx| cx.open_url(&uri)),
                )
            }
            Shown::Starting(_) => None,
            Shown::Error(_) if !configured => None,
            _ => Some(
                microsoft::sign_in_button("owner-sign-in", &t).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.sign_in_to_play();
                        cx.notify();
                    },
                )),
            ),
        };
        let waiting = matches!(state, Shown::Code(..)).then(|| {
            shell::text(theme::night::CAPTION, shell::c(n.muted))
                .relative()
                .child(tr("Waiting for Microsoft… the window goes on by itself.").to_string())
        });
        let answers = div()
            .relative()
            .flex()
            .items_center()
            .gap(px(10.))
            .mt(px(14.))
            .children(main)
            .child(
                shell::action("owner-other", shell::Action::Quiet, &t)
                    .child(shell::label(tr("Other ways to play").to_string()))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.open_provider();
                        cx.notify();
                    })),
            )
            .child(
                shell::action("owner-later", shell::Action::Quiet, &t)
                    .child(shell::label(tr("Not now").to_string()))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.close_owner_screen();
                        cx.notify();
                    })),
            );
        // Opaque: it sits over the page's content, not over the bare sky.
        let card = plate::cut(
            theme::night::PIXEL_CORNER,
            2,
            Some(plate::alpha(n.card, 0.98)),
            Some((plate::alpha(n.stroke, 2. * n.stroke_alpha), 1.)),
        )
        .flex()
        .flex_col()
        .gap(px(8.))
        // A set width: text measured at it wraps where it is drawn.
        .w(px(520.))
        .max_w_full()
        .p(px(32.))
        .child(body)
        .child(answers)
        .children(waiting);
        // The sky stays behind, dimmed: the sheet is over the launcher, not instead of it.
        self.gate("owner-gate", card)
    }

    /// The screen in the glass look: a dialog plate over the dimmed, blurred launcher (the
    /// mockup's modals), its Sign in the Account card's white Microsoft button, its code in the
    /// card's inner panel.
    fn glass_owner_screen(
        &self,
        state: Shown,
        configured: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        use crate::glass_ui::{self as ui, white};
        use horizon_theme::glass::{ink, radius};
        let t = self.theme;
        let panel = || {
            div()
                .mt(px(20.))
                .p(px(20.))
                .rounded(px(radius::POPOVER))
                .bg(white(0.045))
        };
        let mut card = ui::dialog_plate(radius::MODAL)
            .id("owner-card")
            .occlude()
            .flex()
            .flex_col()
            // A set width: text measured at it wraps where it is drawn.
            .w(px(520.))
            .max_w_full()
            .p(px(32.))
            .child(ui::text(13., FontWeight::MEDIUM, ink::QUIET).child(tr("Microsoft account")))
            .child(
                div()
                    .mt(px(8.))
                    .font_family(horizon_theme::glass::DISPLAY_FONT)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(28.))
                    .line_height(px(35.))
                    .text_color(white(ink::FULL))
                    .child(tr("Sign in to play")),
            )
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(ui::text(14., FontWeight::NORMAL, ink::BODY).child(tr(
                        "Horizon needs a Microsoft account that owns Minecraft: Java Edition, like other launchers.",
                    )))
                    .child(ui::text(14., FontWeight::NORMAL, ink::BODY).child(tr(
                        "No Minecraft install needed: Horizon downloads it from Mojang for your account.",
))),
            );
        card = match state {
            Shown::Code(uri, user_code) => {
                let copy = user_code.to_string();
                let (private_uri, link_uri, other_uri) =
                    (uri.to_string(), uri.to_string(), uri.to_string());
                card.child(
                    panel()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(ui::text(13., FontWeight::NORMAL, ink::NOTE).child(trf(
                                    "Open {link} in your browser and enter this code:",
                                    &[("link", short_link(uri))],
                                )))
                                .child(
                                    div()
                                        .font_family(theme::MONO)
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(30.))
                                        .line_height(px(38.))
                                        .text_color(white(ink::FULL))
                                        .child(user_code.to_string()),
                                ),
                        )
                        .child(ui::quiet("owner-copy", tr("Copy code"), 36., &t).on_click(
                            move |_, _, cx| {
                                copy_to_clipboard(cx, gpui::ClipboardItem::new_string(copy.clone()))
                            },
                        )),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(4.))
                        .when(!microsoft::has_private_window(), |col| {
                            col.child(ui::text(13., FontWeight::NORMAL, ink::QUIET).child(tr(
                                "Open this link in a private window to pick another account.",
                            )))
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(if microsoft::has_private_window() {
                                    ui::quiet(
                                        "owner-private",
                                        tr("Open in private window"),
                                        32.,
                                        &t,
                                    )
                                    .on_click(
                                        move |_, _, cx| microsoft::open_private(&private_uri, cx),
                                    )
                                } else {
                                    ui::quiet("owner-copy-link", tr("Copy link"), 32., &t).on_click(
                                        move |_, _, cx| {
                                            copy_to_clipboard(
                                                cx,
                                                gpui::ClipboardItem::new_string(link_uri.clone()),
                                            )
                                        },
                                    )
                                })
                                .child(
                                    ui::quiet(
                                        "owner-other",
                                        tr("Use a different Microsoft account"),
                                        32.,
                                        &t,
                                    )
                                    .on_click(
                                        move |_, _, cx| {
                                            microsoft::use_different_account(&other_uri, cx)
                                        },
                                    ),
                                ),
                        ),
                )
            }
            Shown::Starting(what) => card
                .child(panel().child(ui::text(14., FontWeight::NORMAL, ink::BODY).child(tr(what)))),
            Shown::Error(e) => card.child(
                panel().child(
                    ui::text(14., FontWeight::NORMAL, ink::FULL)
                        .text_color(ui::color(horizon_theme::glass::DANGER))
                        .child(e.to_string()),
                ),
            ),
            Shown::Idle => card,
        };
        let main: Option<AnyElement> = match state {
            Shown::Code(uri, _) => {
                let uri = uri.to_string();
                Some(
                    ui::button(
                        "owner-open".into(),
                        tr("Open link").into(),
                        Kind::Primary,
                        &t,
                    )
                    .h(px(44.))
                    .px(px(24.))
                    .on_click(move |_, _, cx| cx.open_url(&uri))
                    .into_any_element(),
                )
            }
            Shown::Starting(_) => None,
            Shown::Error(_) if !configured => None,
            _ => Some(
                keyboard(
                    div()
                        .id("owner-sign-in")
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(10.))
                        .h(px(44.))
                        .px(px(20.))
                        .rounded(px(radius::ROW))
                        .bg(gpui::white())
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.9))
                        .font_family(horizon_theme::glass::FONT)
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(gpui::rgb(horizon_theme::glass::ON_LIGHT))
                        .child(microsoft::logo(18.))
                        .child(tr("Sign in with Microsoft")),
                    &t,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.sign_in_to_play();
                    cx.notify();
                }))
                .into_any_element(),
            ),
        };
        let later = ui::button("owner-later".into(), tr("Not now").into(), Kind::Ghost, &t)
            .h(px(44.))
            .px(px(20.))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.close_owner_screen();
                cx.notify();
            }));
        // Sign-in not configured, or another provider wanted: the chooser is one click away.
        let other = ui::quiet("owner-other-ways", tr("Ely.by or offline instead"), 44., &t)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.open_provider();
                cx.notify();
            }));
        card = card.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .mt(px(24.))
                .children(main)
                .child(later)
                .child(other),
        );
        if matches!(state, Shown::Code(..)) {
            card = card.child(
                ui::text(13., FontWeight::NORMAL, ink::QUIET)
                    .mt(px(12.))
                    .child(tr("Waiting for Microsoft… the window goes on by itself.")),
            );
        }
        div()
            .id("owner-gate")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .occlude()
            .child(ui::scrim_backdrop())
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(ui::scrim()),
            )
            .child(card)
    }

    /// The ways to play in the glass look, as the sign-in sheet: Microsoft's white button, then
    /// Ely.by and offline, over the dimmed launcher.
    fn glass_provider_screen(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        use crate::glass_ui::{self as ui, white};
        use horizon_theme::glass::{ink, radius};
        let t = self.theme;
        let microsoft = keyboard(
            div()
                .id("provider-microsoft")
                .flex()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .h(px(44.))
                .w_full()
                .rounded(px(radius::ROW))
                .bg(gpui::white())
                .when(ui::light(), |d| d.border_1().border_color(ui::white(0.15)))
                .cursor_pointer()
                .hover(|s| s.opacity(0.9))
                .font_family(horizon_theme::glass::FONT)
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(gpui::rgb(horizon_theme::glass::ON_LIGHT))
                .child(microsoft::logo(18.))
                .child(tr("Microsoft account")),
            &t,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.provider_open = false;
            this.owner_screen_open = true;
            this.start_sign_in();
            cx.notify();
        }));
        let card = ui::dialog_plate(radius::MODAL)
            .id("provider-card")
            .occlude()
            .flex()
            .flex_col()
            .w(px(520.))
            .max_w_full()
            .p(px(32.))
            .child(ui::text(13., FontWeight::MEDIUM, ink::QUIET).child(tr("Account")))
            .child(
                div()
                    .mt(px(8.))
                    .font_family(horizon_theme::glass::DISPLAY_FONT)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(28.))
                    .line_height(px(35.))
                    .text_color(white(ink::FULL))
                    .child(tr("Choose how to play")),
            )
            .child(
                ui::text(14., FontWeight::NORMAL, ink::BODY)
                    .mt(px(12.))
                    .line_height(px(21.))
                    .child(tr(
                        "Use a Microsoft account or an Ely.by account. You can change this later in \
                         Settings.",
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .mt(px(24.))
                    .child(microsoft)
                    .child(
                        ui::button(
                            "provider-elyby".into(),
                            tr("Ely.by account").into(),
                            Kind::Secondary,
                            &t,
                        )
                        .w_full()
                        .h(px(44.))
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.provider_open = false;
                                this.ely_screen_open = true;
                                this.ely_state = None;
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        ui::button(
                            "provider-offline".into(),
                            tr("Play offline").into(),
                            Kind::Secondary,
                            &t,
                        )
                        .w_full()
                        .h(px(44.))
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.choose_offline();
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(
                ui::text(13., FontWeight::NORMAL, ink::QUIET)
                    .mt(px(16.))
                    .line_height(px(19.5))
                    .child(tr(
                        "Offline plays single player and LAN worlds under the game's own name \
                         (online servers need an account).",
                    )),
            )
            .child(div().flex().justify_end().mt(px(8.)).child(
                ui::quiet("provider-later", tr("Not now"), 40., &t).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.provider_open = false;
                        this.settings.owner_notice_seen = true;
                        let _ = this.settings.save();
                        cx.notify();
                    },
                )),
            ));
        div()
            .id("provider-gate")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .occlude()
            .child(ui::scrim_backdrop())
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(ui::scrim()),
            )
            .child(card)
    }

    /// The Ely.by sign-in in the glass look: its login, password and two-factor code, in the
    /// sign-in sheet's plate.
    fn glass_ely_screen(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        use crate::glass_ui::{self as ui, white};
        use horizon_theme::glass::{ink, radius};
        let t = self.theme;
        let login = ui::wide_field(
            self.ely_field(
                "elyby-login",
                &self.ely_login,
                self.ely_login_focused,
                &self.ely_login_focus,
                "Ely.by login or e-mail",
                false,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                window.focus(&this.ely_login_focus, cx);
                this.ely_login_focused = true;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match line_edit::handle(&mut this.ely_login, ev, cx) {
                    line_edit::Key::Pass => return,
                    line_edit::Key::Escape => window.blur(cx),
                    _ => {}
                }
                cx.stop_propagation();
                cx.notify();
            })),
            self.ely_login_focused,
        );
        let password = ui::wide_field(
            self.ely_field(
                "elyby-password",
                &self.ely_password,
                self.ely_password_focused,
                &self.ely_password_focus,
                "Password",
                true,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                window.focus(&this.ely_password_focus, cx);
                this.ely_password_focused = true;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match line_edit::handle(&mut this.ely_password, ev, cx) {
                    line_edit::Key::Pass => return,
                    line_edit::Key::Escape => window.blur(cx),
                    _ => {}
                }
                cx.stop_propagation();
                cx.notify();
            })),
            self.ely_password_focused,
        );
        let two_factor = self.ely_state == Some(ElySignInState::TwoFactor);
        let code = two_factor.then(|| {
            ui::wide_field(
                self.ely_field(
                    "elyby-code",
                    &self.ely_code,
                    self.ely_code_focused,
                    &self.ely_code_focus,
                    "Two-factor code",
                    false,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    window.focus(&this.ely_code_focus, cx);
                    this.ely_code_focused = true;
                    cx.notify();
                }))
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    match line_edit::handle(&mut this.ely_code, ev, cx) {
                        line_edit::Key::Pass => return,
                        line_edit::Key::Escape => window.blur(cx),
                        _ => {}
                    }
                    cx.stop_propagation();
                    cx.notify();
                })),
                self.ely_code_focused,
            )
        });
        let busy = matches!(self.ely_state, Some(ElySignInState::Starting));
        let sign_in = ui::button(
            "elyby-sign-in".into(),
            if busy {
                tr("Signing in…").into()
            } else {
                tr("Sign in").into()
            },
            Kind::Primary,
            &t,
        )
        .h(px(44.));
        let sign_in = if busy {
            sign_in.opacity(0.6)
        } else {
            sign_in.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.ely_sign_in_pressed();
                cx.notify();
            }))
        };
        let mut card = ui::dialog_plate(radius::MODAL)
            .id("ely-card")
            .occlude()
            .flex()
            .flex_col()
            .w(px(520.))
            .max_w_full()
            .p(px(32.))
            .child(ui::text(13., FontWeight::MEDIUM, ink::QUIET).child(tr("Ely.by account")))
            .child(
                div()
                    .mt(px(8.))
                    .font_family(horizon_theme::glass::DISPLAY_FONT)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(28.))
                    .line_height(px(35.))
                    .text_color(white(ink::FULL))
                    .child(tr("Sign in to play")),
            )
            .child(
                ui::text(14., FontWeight::NORMAL, ink::BODY)
                    .mt(px(12.))
                    .line_height(px(21.))
                    .child(tr(
                        "Your password goes only to authserver.ely.by. The launcher keeps the \
                         token, never the password.",
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .mt(px(20.))
                    .child(login)
                    .child(password),
            );
        if let Some(code) = code {
            card = card.child(code);
        }
        if let Some(ElySignInState::Done(Err(e))) = &self.ely_state {
            card = card.child(
                ui::text(14., FontWeight::NORMAL, ink::FULL)
                    .mt(px(12.))
                    .text_color(ui::color(horizon_theme::glass::DANGER))
                    .child(e.clone()),
            );
        } else if let Some(ElySignInState::Starting) = &self.ely_state {
            card = card.child(
                ui::text(14., FontWeight::NORMAL, ink::BODY)
                    .mt(px(12.))
                    .child(tr("Signing in to Ely.by…")),
            );
        } else if two_factor {
            card = card.child(
                ui::text(13., FontWeight::NORMAL, ink::QUIET)
                    .mt(px(12.))
                    .child(tr("Enter the two-factor code from your authenticator.")),
            );
        }
        card = card.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .mt(px(24.))
                .child(sign_in)
                .child(
                    ui::quiet("elyby-back", tr("Cancel"), 44., &t).on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.ely_cancel();
                            this.open_provider();
                            cx.notify();
                        },
                    )),
                ),
        );
        div()
            .id("ely-gate")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .occlude()
            .child(ui::scrim_backdrop())
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(ui::scrim()),
            )
            .child(card)
    }

    /// Sends the Ely.by sign-in with what the fields hold, as the sheet's Sign in does.
    fn ely_sign_in_pressed(&mut self) {
        let login = self.ely_login.text().trim().to_string();
        let password = self.ely_password.text().to_string();
        if login.is_empty() || password.is_empty() {
            self.ely_state = Some(ElySignInState::Done(Err(
                "Enter your Ely.by login and password.".into(),
            )));
            return;
        }
        let code = if self.ely_state == Some(ElySignInState::TwoFactor) {
            let code = self.ely_code.text().trim().to_string();
            if code.is_empty() {
                self.ely_state = Some(ElySignInState::Done(Err(
                    "Enter the two-factor code.".into()
                )));
                return;
            }
            Some(code)
        } else {
            None
        };
        self.ely_state = Some(ElySignInState::Starting);
        self.ely_sign_in = Some(ElySignIn::start(login, password, code));
    }

    /// Closes the Ely.by sheet and forgets what was typed there, but for the login.
    pub(crate) fn ely_cancel(&mut self) {
        if let Some(sign_in) = &self.ely_sign_in {
            sign_in.cancel();
        }
        self.ely_sign_in = None;
        self.ely_screen_open = false;
        self.ely_state = None;
        self.ely_password.set(String::new());
        self.ely_code.set(String::new());
    }
}

/// What the sheet shows of the sign-in.
#[derive(Clone, Copy)]
enum Shown<'a> {
    Idle,
    /// What the sign-in is doing.
    Starting(&'static str),
    /// The page and the code to enter there.
    Code(&'a str, &'a str),
    Error(&'a str),
}

/// `HORIZON_LAUNCHER_PAGE=sign-in` opens the sheet with a made-up code, for screenshots: nothing
/// asks Microsoft, and the gate itself is as it was (no account, no Play).
pub(super) fn sign_in_preview() -> bool {
    page_preview("sign-in")
}

/// `HORIZON_LAUNCHER_PAGE=choose` (or `elyby`) opens that sheet the same way, for screenshots.
pub(super) fn page_preview(page: &str) -> bool {
    std::env::var("HORIZON_LAUNCHER_PAGE").as_deref() == Ok(page)
}

/// `https://www.microsoft.com/link` as `microsoft.com/link`.
fn short_link(uri: &str) -> &str {
    let uri = uri
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    uri.strip_prefix("www.").unwrap_or(uri)
}

#[cfg(test)]
mod tests {
    #[test]
    fn links_are_shown_as_typed() {
        assert_eq!(
            super::short_link("https://www.microsoft.com/link"),
            "microsoft.com/link"
        );
        assert_eq!(
            super::short_link("https://login.live.com/oauth20_remoteconnect.srf"),
            "login.live.com/oauth20_remoteconnect.srf"
        );
    }
}
