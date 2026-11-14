//! Shared rectangular shell. Content pages keep their existing layouts and services.
use super::*;
use iced::widget::{Space, column, row, rule};

pub(super) fn primary_button(
    theme: &iced::Theme,
    status: iced_button::Status,
) -> iced_button::Style {
    let mut style = iced_button::primary(theme, status);
    style.border.radius = 0.into();
    style
}

pub(super) fn navigation_button(
    theme: &iced::Theme,
    status: iced_button::Status,
    active: bool,
) -> iced_button::Style {
    let mut style = if active {
        iced_button::primary(theme, status)
    } else {
        iced_button::secondary(theme, status)
    };
    style.border.radius = 0.into();
    style
}

fn chrome(theme: &iced::Theme, opacity: f32, footer: bool) -> container::Style {
    let dark = theme.palette().background.r < 0.5;
    let mut color = match (dark, footer) {
        (false, false) => iced::Color::from_rgb8(232, 238, 246),
        (false, true) => iced::Color::from_rgb8(241, 244, 248),
        (true, false) => iced::Color::from_rgb8(33, 43, 56),
        (true, true) => iced::Color::from_rgb8(27, 35, 46),
    };
    color.a = opacity;
    container::Style::default().background(color)
}

fn separator(theme: &iced::Theme) -> rule::Style {
    rule::Style {
        color: if theme.palette().background.r < 0.5 {
            iced::Color::from_rgb8(67, 82, 101)
        } else {
            iced::Color::from_rgb8(187, 199, 215)
        },
        ..rule::default(theme)
    }
}

pub(super) fn pick_list_style(
    theme: &iced::Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    let mut style = iced::widget::pick_list::default(theme, status);
    style.border.radius = 0.into();
    style
}

pub(super) fn toolbar(app: &App) -> Element<'_, Message> {
    let actions = row![
        button("功能")
            .on_press(Message::ToggleModules)
            .style(move |theme, status| navigation_button(
                theme,
                status,
                app.section == Section::Modules
            )),
        button("设置")
            .on_press(Message::Navigate(Section::Settings(Settings::Page)))
            .style(move |theme, status| navigation_button(
                theme,
                status,
                matches!(app.section, Section::Settings(_))
            )),
        button("收起")
            .on_press(Message::HideWindow)
            .style(|theme, status| navigation_button(theme, status, false))
    ]
    .spacing(8);
    let refresh = (app.section == Section::Modules)
        .then(|| button("刷新").on_press_maybe((!app.manager_busy).then_some(Message::Reload)));
    let actions = actions.align_y(iced::Center);
    let title = iced::widget::mouse_area(text("LazyBuddy").size(22)).on_press(Message::Drag);
    let select = pick_list(app.roots.clone(), app.selected.clone(), Message::Select)
        .placeholder("选择功能")
        .width(160)
        .style(pick_list_style);
    if app.width < 640. {
        let mut selection = row![select, Space::new().width(Fill)];
        if let Some(refresh) = refresh {
            selection = selection.push(refresh);
        }
        column![
            row![title, Space::new().width(Fill), actions].align_y(iced::Center),
            selection
        ]
        .spacing(10)
        .into()
    } else {
        let mut toolbar = row![title, select, Space::new().width(Fill)];
        if let Some(refresh) = refresh {
            toolbar = toolbar.push(refresh);
        }
        toolbar
            .push(actions)
            .spacing(16)
            .align_y(iced::Center)
            .into()
    }
}

pub(super) fn page_scroll<'a>(
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Scrollable<'a, Message> {
    scrollable(content)
        .height(Fill)
        .width(Fill)
        .style(|theme, status| {
            let mut style = scrollable::default(theme, status);
            style.vertical_rail.border.radius = 0.into();
            style.vertical_rail.scroller.border.radius = 0.into();
            style
        })
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(8)
                .scroller_width(8)
                .spacing(12),
        ))
}

pub(super) fn modules(app: &App) -> Element<'_, Message> {
    let mut list = column![].spacing(0);
    for entry in crate::modules::feature_entries(&app.modules, app.catalog.as_ref()) {
        let mut actions = row![].spacing(8);
        if entry.installed {
            actions = actions.push(button("更新").on_press_maybe(
                (entry.update && !app.manager_busy).then(|| Message::Install(entry.id.into())),
            ));
            actions =
                actions.push(button("卸载").on_press_maybe(
                    (!app.manager_busy).then(|| Message::Uninstall(entry.id.into())),
                ));
        } else {
            actions = actions
                .push(button("安装").on_press_maybe(
                    (!app.manager_busy).then(|| Message::Install(entry.id.into())),
                ));
        }
        list = list.push(
            container(
                row![
                    row![text(entry.name).size(16), text(entry.version).size(14)]
                        .spacing(10)
                        .align_y(iced::Center)
                        .width(Fill),
                    actions,
                ]
                .spacing(16)
                .align_y(iced::Center),
            )
            .padding([12, 0])
            .width(Fill),
        );
        list = list.push(rule::horizontal(1).style(separator));
    }
    column![text("可用功能").size(24), page_scroll(list)]
        .spacing(16)
        .height(Fill)
        .into()
}

fn status_text(app: &App) -> String {
    let mut messages = Vec::new();
    if let Some(error) = &app.error {
        messages.push(error.as_str());
    }
    if app.manager_busy {
        messages.push(if app.catalog.is_none() {
            "正在读取功能目录…"
        } else {
            "正在处理功能包…"
        });
    }
    if app.busy
        && !matches!(
            app.current_method.as_str(),
            "cover.read" | "media.status" | "account.status"
        )
    {
        messages.push("加载中…");
    }
    if matches!(
        app.section,
        Section::Content | Section::Settings(Settings::Page)
    ) {
        if let Some(page) = app.page() {
            match page.account["state"].as_str() {
                Some("preparing") => messages.push("正在获取二维码…"),
                Some("verifying") => messages.push("正在核对身份…"),
                Some("pending") => messages.push("等待扫码登录…"),
                Some("scanned") => messages.push("已扫码，请在手机上确认…"),
                _ => {}
            }
            if page.account["operation"] == "pending" {
                messages.push("正在同步账号数据…");
            }
            if page.cards.iter().any(|card| card["ok"] == false) {
                messages.push("部分新闻来源刷新失败，请重试。");
            }
            if page.account["sync_error"].is_string() {
                messages.push("云端进度同步失败，本地记录已保留。");
            }
            if page.account["favorites_error"].is_string() {
                messages.push("收藏读取失败，保留上次确认结果。");
            }
        }
    }
    if messages.is_empty() {
        "就绪".into()
    } else {
        messages.join("  ·  ")
    }
}

pub(super) fn frame<'a>(
    app: &'a App,
    toolbar: Element<'a, Message>,
    page: Element<'a, Message>,
) -> Element<'a, Message> {
    let opacity = app.preferences.background_opacity;
    let toolbar = container(toolbar)
        .padding([12, 20])
        .width(Fill)
        .style(move |theme| chrome(theme, opacity, false));
    let page = container(page).padding(20).width(Fill).height(Fill);
    let status = container(
        text(status_text(app))
            .size(12)
            .color(if app.error.is_some() {
                if app.theme().palette().background.r < 0.5 {
                    iced::Color::from_rgb8(255, 160, 150)
                } else {
                    iced::Color::from_rgb8(170, 35, 35)
                }
            } else {
                app.theme().palette().text
            }),
    )
    .padding([8, 20])
    .width(Fill)
    .style(move |theme| chrome(theme, opacity, true));
    container(
        column![
            toolbar,
            rule::horizontal(2).style(separator),
            page,
            rule::horizontal(1).style(separator),
            status
        ]
        .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(move |theme: &iced::Theme| {
        container::Style::default().background(iced::Color {
            a: opacity,
            ..theme.palette().background
        })
    })
    .into()
}
