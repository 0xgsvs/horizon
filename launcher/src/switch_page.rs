//! Who plays: the top bar's quick switch (a menu under the account chip) between the signed-in
//! Microsoft and Ely.by accounts.
use super::*;
use horizon_launcher::account::Player;

/// The name the offline player has in the game: the one it made up and kept; `Offline` until
/// the game has run once.
fn offline_name() -> String {
    account::offline_game_name(&backend::game_home()).unwrap_or_else(|| tr("Offline").to_string())
}

impl Launcher {
    /// The chip's two lines: the name that plays, and how.
    pub(super) fn who_plays(&self) -> (String, &'static str) {
        match self.accounts.player() {
            Some(Player::Microsoft(_)) => (
                self.accounts
                    .selected()
                    .map_or_else(String::new, |a| a.name.clone()),
                tr("Microsoft account"),
            ),
            Some(Player::ElyBy(_)) => (
                self.accounts
                    .elyby_selected()
                    .map_or_else(String::new, |a| a.name.clone()),
                tr("Ely.by account"),
            ),
            Some(Player::Offline) => (offline_name(), tr("Offline")),
            None => (String::new(), tr("Sign in with Microsoft")),
        }
    }

    pub(super) fn play_as(&mut self, who: &Player) {
        let mut accounts = Accounts::load();
        let done = accounts.play_as(who.clone()).and_then(|()| accounts.save());
        match done {
            Ok(()) => self.accounts = accounts,
            Err(e) => self.sign_in_state = Some(SignInState::Done(Err(e))),
        }
        self.account_menu = false;
    }

    /// Everyone the player can play as (each account's UUID and name), with how each signs in.
    pub(super) fn players(&self) -> Vec<(Player, String, &'static str)> {
        let mut out: Vec<(Player, String, &'static str)> = self
            .accounts
            .accounts
            .iter()
            .map(|a| {
                (
                    Player::Microsoft(a.uuid.clone()),
                    a.name.clone(),
                    tr("Microsoft"),
                )
            })
            .collect();
        out.extend(
            self.accounts
                .elyby
                .iter()
                .map(|a| (Player::ElyBy(a.uuid.clone()), a.name.clone(), tr("Ely.by"))),
        );
        // Offline plays beside the accounts: the same menu switches to it and back.
        out.push((Player::Offline, offline_name(), tr("Offline")));
        out
    }

    /// The menu under the account chip, while open.
    pub(super) fn account_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.account_menu {
            return None;
        }
        let t = self.theme;
        let now = self.accounts.player();
        let mut menu = div()
            .id("account-menu")
            .absolute()
            .top(px(46.))
            .right_0()
            .w(px(280.))
            .p(px(6.))
            .border_2()
            .border_color(t.outline)
            .bg(t.surface)
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                this.account_menu = false;
                this.menu_closed_at = Some(event.position);
                cx.notify();
            }));
        for (i, (who, name, how)) in self.players().into_iter().enumerate() {
            let chosen = now.as_ref() == Some(&who);
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("account-pick-{i}")))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .h(px(46.))
                    .px(px(10.))
                    .rounded(px(theme::RADIUS))
                    .cursor_pointer()
                    .hover(move |s| s.bg(t.plate_soft))
                    .child(
                        self.profile_heads
                            .draw(Some(who.uuid()), 24., self.scale, &t),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(text(theme::LABEL).truncate().text_color(t.ink).child(name))
                            .child(text(theme::CAPTION).text_color(t.muted).child(how)),
                    )
                    .when(chosen, |d| d.child(div().size(px(6.)).bg(t.accent)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.play_as(&who);
                        cx.notify();
                    })),
            );
        }
        menu = menu.child(div().my(px(6.)).h(px(1.)).bg(t.rule)).child(
            div()
                .id("account-add")
                .flex()
                .items_center()
                .h(px(32.))
                .px(px(10.))
                .rounded(px(theme::RADIUS))
                .text_size(px(theme::LABEL.px))
                .text_color(t.muted)
                .cursor_pointer()
                .hover(move |s| s.bg(t.plate_soft).text_color(t.ink))
                .child(if account::client_id().is_some() {
                    tr("Sign in with Microsoft")
                } else {
                    tr("Ely.by account or play offline")
                })
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.account_menu = false;
                    if account::client_id().is_some() {
                        this.start_sign_in();
                    } else {
                        this.open_provider();
                    }
                    cx.notify();
                })),
        );
        menu = menu.child(div().my(px(6.)).h(px(1.)).bg(t.rule)).child(
            div()
                .id("account-manage")
                .flex()
                .items_center()
                .h(px(32.))
                .px(px(10.))
                .rounded(px(theme::RADIUS))
                .text_size(px(theme::LABEL.px))
                .text_color(t.muted)
                .cursor_pointer()
                .hover(move |s| s.bg(t.plate_soft).text_color(t.ink))
                .child(tr("Manage accounts"))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.account_menu = false;
                    this.go(Page::Settings);
                    cx.notify();
                })),
        );
        Some(gpui::deferred(menu).with_priority(1))
    }

    /// Settings' Ely.by accounts: each with Play as and Sign out, then Add account.
    pub(super) fn elyby_rows(&mut self, mut n: usize, cx: &mut Context<Self>) -> (Div, usize) {
        let t = self.theme;
        let mut rows = div().flex().flex_col();
        let now = self.accounts.player();
        for (i, account) in self.accounts.elyby.clone().into_iter().enumerate() {
            let plays = now == Some(Player::ElyBy(account.uuid.clone()));
            let mut actions = div().flex().gap(px(4.));
            if !plays {
                let uuid = account.uuid.clone();
                actions = actions.child(
                    button(format!("elyby-play-{i}"), "Play as", Kind::Secondary, &t).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let who = Player::ElyBy(uuid.clone());
                            this.play_as(&who);
                            cx.notify();
                        }),
                    ),
                );
            }
            let uuid = account.uuid.clone();
            actions = actions.child(
                button(format!("elyby-sign-out-{i}"), "Sign out", Kind::Ghost, &t).on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let mut accounts = Accounts::load();
                        if accounts
                            .remove_elyby(account::store(), &uuid)
                            .and_then(|()| accounts.save())
                            .is_ok()
                        {
                            this.accounts = accounts;
                        }
                        cx.notify();
                    }),
                ),
            );
            n += 1;
            rows = rows.child(slot_row(
                None,
                account.name,
                Some(
                    div()
                        .child(tr(if plays {
                            "Ely.by account · plays"
                        } else {
                            "Ely.by account"
                        }))
                        .into_any_element(),
                ),
                Some(actions.into_any_element()),
                &t,
            ));
        }
        n += 1;
        rows = rows.child(index_row(
            n,
            tr("Ely.by account"),
            Some(
                div()
                    .child(tr(
                        "Sign in with your Ely.by login. Online servers can use the account when they accept Ely.by.",
                    ))
                    .into_any_element(),
            ),
            Some(
                button("elyby-add", "Add account", Kind::Secondary, &t)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.ely_screen_open = true;
                        this.ely_state = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            ),
            &t,
        ));
        (rows, n)
    }
}
