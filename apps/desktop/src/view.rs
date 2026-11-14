mod player;
use super::*;
use iced::{
    Fill,
    widget::{
        button, checkbox, column, container, image, pick_list, row, scrollable, text, text_input,
    },
};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    if app.capsule {
        return container(
            button(text("zZ").size(16))
                .width(Fill)
                .height(Fill)
                .on_press(Message::Expand),
        )
        .center(Fill)
        .style(|theme: &iced::Theme| {
            container::Style::default()
                .background(theme.palette().background)
                .border(iced::Border {
                    radius: 16.into(),
                    ..Default::default()
                })
        })
        .into();
    }
    let list_id = app.list_id();
    let generation = app.play_generation;
    let header: Element<'_, Message> = if app.width < 640. {
        column![
            row![
                iced::widget::mouse_area(container(text("LazyBuddy").size(22)).width(Fill))
                    .on_press(Message::Drag),
                button("功能").on_press(Message::Navigate(Section::Modules)),
                button("设置").on_press(Message::Navigate(Section::Settings(Settings::Page))),
                button("收起").on_press(Message::HideWindow)
            ]
            .spacing(8)
            .align_y(iced::Center),
            pick_list(app.roots.clone(), app.selected.clone(), Message::Select)
                .placeholder("选择功能")
                .width(Fill)
        ]
        .spacing(10)
        .into()
    } else {
        row![
            iced::widget::mouse_area(text("LazyBuddy").size(22)).on_press(Message::Drag),
            pick_list(app.roots.clone(), app.selected.clone(), Message::Select)
                .placeholder("选择功能"),
            button("功能").on_press(Message::Navigate(Section::Modules)),
            button("设置").on_press(Message::Navigate(Section::Settings(Settings::Page))),
            button("收起").on_press(Message::HideWindow)
        ]
        .spacing(12)
        .align_y(iced::Center)
        .into()
    };
    let mut body = column![header].spacing(16);
    if let Some(error) = &app.error {
        body = body.push(text(error));
    }
    let content: Element<'_, Message> = match app.section {
        Section::Modules => {
            let mut modules = column![
                text("按需安装的功能").size(24),
                button("刷新").on_press_maybe((!app.manager_busy).then_some(Message::Reload))
            ]
            .spacing(12);
            for module in &app.modules {
                modules = modules.push(
                    row![
                        text(format!("{}  {}", module.name, module.version)).width(Fill),
                        button("卸载").on_press_maybe(
                            (!app.manager_busy).then(|| Message::Uninstall(module.id.clone()))
                        )
                    ]
                    .spacing(12),
                );
            }
            if app.modules.is_empty() {
                modules = modules.push(text("尚未安装功能。"));
            }
            if app.manager_busy {
                modules = modules.push(text("正在处理功能包…"));
            }
            if let Some(catalog) = &app.catalog {
                modules = modules.push(text("可用功能").size(22));
                for package in catalog
                    .packages
                    .iter()
                    .filter(|package| package.kind == Kind::Feature)
                {
                    let version = app.installed.get(&package.id);
                    let current = version == Some(&package.version);
                    let bytes = catalog
                        .installation_order(&package.id)
                        .ok()
                        .into_iter()
                        .flatten()
                        .filter(|package| app.installed.get(&package.id) != Some(&package.version))
                        .map(|package| package.bytes)
                        .sum::<u64>();
                    let label = if current {
                        "已安装"
                    } else if version.is_some() {
                        "更新"
                    } else {
                        "安装"
                    };
                    let name = if package.name.is_empty() {
                        &package.id
                    } else {
                        &package.name
                    };
                    modules = modules.push(
                        row![
                            column![
                                text(name).size(18),
                                text(format!(
                                    "{} · 下载约 {:.1} MB",
                                    package.version,
                                    bytes as f64 / 1048576.
                                ))
                            ]
                            .width(Fill),
                            button(label).on_press_maybe(
                                (!current && !app.manager_busy)
                                    .then(|| Message::Install(package.id.clone()))
                            )
                        ]
                        .spacing(12),
                    );
                }
            }
            modules.into()
        }
        Section::Settings(category) => {
            let mut categories = column![].spacing(8).width(140);
            if app.selected.is_some() {
                categories = categories.push(
                    button("当前功能")
                        .on_press(Message::Navigate(Section::Settings(Settings::Page))),
                );
            }
            for (label, category) in [
                ("显示设置", Settings::Display),
                ("系统设置", Settings::System),
                ("关于我们", Settings::About),
            ] {
                categories = categories
                    .push(button(label).on_press(Message::Navigate(Section::Settings(category))));
            }
            categories =
                categories.push(button("返回").on_press(Message::Navigate(Section::Content)));
            let settings: Element<'_, Message> = match category {
                Settings::Display => column![
                    text("显示设置").size(24),
                    checkbox(app.preferences.dark)
                        .label("深色主题")
                        .on_toggle(Message::Dark),
                    checkbox(app.preferences.topmost)
                        .label("窗口置顶")
                        .on_toggle(Message::Topmost),
                    text("背景透明度"),
                    iced::widget::slider(
                        0.0..=1.,
                        app.preferences.background_opacity,
                        Message::BackgroundOpacity
                    )
                    .step(0.01_f32)
                    .on_release(Message::SavePreferences),
                    text("卡片与视频透明度"),
                    iced::widget::slider(
                        0.0..=1.,
                        app.preferences.card_opacity,
                        Message::CardOpacity
                    )
                    .step(0.01_f32)
                    .on_release(Message::SavePreferences)
                ]
                .spacing(16)
                .into(),
                Settings::System => column![
                    text("系统设置").size(24),
                    checkbox(app.autostart)
                        .label("开机启动（后台待命）")
                        .on_toggle_maybe(
                            (!lazybuddy_host_control::isolated()).then_some(Message::Autostart)
                        ),
                    row![
                        text(format!("全局快捷键：{}", app.preferences.shortcut.label())),
                        button(if app.shortcut_recording {
                            "取消"
                        } else {
                            "修改"
                        })
                        .on_press_maybe(
                            (!lazybuddy_host_control::isolated()).then_some(
                                if app.shortcut_recording {
                                    Message::CancelShortcut
                                } else {
                                    Message::CaptureShortcut
                                }
                            )
                        )
                    ]
                    .spacing(12),
                    text(if app.shortcut_recording {
                        "按下 Ctrl 或 Alt 组合键，Esc 取消"
                    } else {
                        "可加 Shift，支持字母、数字及功能键"
                    }),
                    text(
                        if app
                            .desktop
                            .as_ref()
                            .is_some_and(|desktop| desktop.hotkey_registered)
                        {
                            "快捷键已启用"
                        } else {
                            "快捷键未注册或当前处于隔离模式"
                        }
                    )
                ]
                .spacing(12)
                .into(),
                Settings::About => column![
                    text("LazyBuddy").size(28),
                    text("你的桌面搭子"),
                    text(env!("CARGO_PKG_VERSION")),
                    text("2.0 开发版，正式更新与下载尚未开放。"),
                    button("项目与开发进展").on_press(Message::OpenLink(
                        "https://github.com/LazyBuddyDev/LazyBuddy/tree/main"
                            .into()
                    ))
                ]
                .spacing(12)
                .into(),
                Settings::Page => match app.template() {
                    Some(Page::MediaLibrary { .. }) => {
                        let default = app
                            .selected
                            .as_ref()
                            .and_then(|root| app.preferences.pages.get(&root.id))
                            .is_some_and(|page| page.default_favorites);
                        column![
                            account(app),
                            checkbox(default)
                                .label("默认打开收藏")
                                .on_toggle(Message::DefaultFavorites)
                        ]
                        .spacing(16)
                        .into()
                    }
                    Some(Page::NewsGrid { .. }) => news_settings(app),
                    None => text("请先选择一个功能。").into(),
                },
            };
            let settings = if category == Settings::Page {
                settings
            } else {
                scrollable(settings).height(Fill).into()
            };
            if app.width < 640. {
                column![
                    row![
                        pick_list(
                            vec![
                                Settings::Page,
                                Settings::Display,
                                Settings::System,
                                Settings::About
                            ],
                            Some(category),
                            |category| Message::Navigate(Section::Settings(category))
                        ),
                        button("返回").on_press(Message::Navigate(Section::Content))
                    ]
                    .spacing(12),
                    settings
                ]
                .spacing(12)
                .into()
            } else {
                row![categories, container(settings).width(Fill)]
                    .spacing(24)
                    .into()
            }
        }
        Section::Content => match (app.template(), app.page()) {
            (Some(Page::NewsGrid { .. }), Some(page)) => {
                let mut categories = vec!["全部分类".to_owned()];
                for category in page
                    .catalog
                    .iter()
                    .filter_map(|source| source["category"].as_str())
                {
                    if !categories.iter().any(|old| old == category) {
                        categories.push(category.to_owned());
                    }
                }
                let filters = row![
                    checkbox(page.followed_only)
                        .label("只看关注")
                        .on_toggle(Message::Followed),
                    pick_list(
                        categories,
                        Some(page.category.clone().unwrap_or_else(|| "全部分类".into())),
                        Message::Category
                    ),
                    button("刷新").on_press_maybe((!app.busy).then_some(Message::Refresh))
                ]
                .spacing(12)
                .align_y(iced::Center);
                let columns = ((app.width - 40.) / 320.).floor().clamp(1., 4.);
                let width = ((app.width - 40. - (columns - 1.) * 12.) / columns).max(240.);
                let mut cards = row![].spacing(12);
                let mut sources: Vec<_> = page
                    .catalog
                    .iter()
                    .filter(|source| {
                        !contains(&page.news_preferences["hidden"], &source["id"])
                            && (!page.followed_only
                                || contains(&page.news_preferences["followed"], &source["id"]))
                            && page
                                .category
                                .as_ref()
                                .is_none_or(|category| source["category"] == category.as_str())
                    })
                    .collect();
                let order = page.news_preferences["order"].as_array();
                sources.sort_by_key(|source| {
                    order
                        .and_then(|order| order.iter().position(|id| id == &source["id"]))
                        .unwrap_or(usize::MAX)
                });
                for source in sources {
                    let card = page.cards.iter().find(|card| card["id"] == source["id"]);
                    let mut content = column![
                        button(text(source["name"].as_str().unwrap_or("新闻")).size(18))
                            .on_press_maybe(
                                (!app.busy)
                                    .then(|| Message::Fetch(source["id"].as_str().unwrap().into()))
                            )
                    ]
                    .spacing(8);
                    if let Some(card) = card {
                        if card["ok"] == false {
                            content = content.push(text("刷新失败，请重试。"));
                        }
                        for (index, story) in
                            card["stories"].as_array().into_iter().flatten().enumerate()
                        {
                            if let (Some(title), Some(url)) =
                                (story["title"].as_str(), story["url"].as_str())
                            {
                                content = content.push(
                                    button(text(format!("{}. {title}", index + 1)).size(14))
                                        .style(button::text)
                                        .on_press(Message::OpenLink(url.into())),
                                );
                            }
                        }
                    } else {
                        content = content.push(text("尚未加载"));
                    }
                    cards = cards.push(
                        container(scrollable(content).height(310))
                            .padding(12)
                            .style(move |theme| card_style(theme, app.preferences.card_opacity))
                            .width(width),
                    );
                }
                column![
                    filters,
                    scrollable(cards.wrap())
                        .id(iced::widget::Id::from(list_id.clone()))
                        .on_scroll(move |viewport| Message::Scroll(
                            list_id.clone(),
                            generation,
                            viewport
                        ))
                        .height(Fill)
                ]
                .spacing(12)
                .into()
            }
            (Some(Page::MediaLibrary { .. }), Some(page)) if page.player_visible => {
                player::view(app, page)
            }
            (Some(Page::MediaLibrary { .. }), Some(page))
                if page.account["state"] == "signed_in" =>
            {
                let tabs = row![
                    button("搜索").on_press(Message::Favorites(false)),
                    button("收藏").on_press(Message::Favorites(true))
                ]
                .spacing(8);
                let mut library = column![tabs].spacing(12);
                if page.account["sync_error"].is_string() {
                    library = library.push(
                        row![
                            text("云端进度同步失败，本地记录已保留。"),
                            button("重试同步").on_press_maybe(
                                (app.can_interact() && page.account["operation"] != "pending")
                                    .then_some(Message::Account("progress.sync"))
                            )
                        ]
                        .spacing(8),
                    );
                }
                if page.account["favorites_error"].is_string() {
                    library = library.push(
                        row![
                            text("收藏读取失败，保留上次确认结果。"),
                            button("重新核对").on_press_maybe(
                                (app.can_interact() && page.account["operation"] != "pending")
                                    .then_some(Message::Account("favorites.read"))
                            )
                        ]
                        .spacing(8),
                    );
                }
                if !page.favorites_tab {
                    library = library.push(
                        row![
                            text_input("搜索短剧", &page.query)
                                .on_input(Message::Query)
                                .on_submit(Message::Search),
                            button("搜索")
                                .on_press_maybe(app.can_interact().then_some(Message::Search))
                        ]
                        .spacing(8),
                    );
                }
                let mut list = column![].spacing(12);
                let rows = if page.favorites_tab {
                    &page.favorites
                } else {
                    &page.cards
                };
                for item in rows {
                    let mut details = column![
                        text(item["title"].as_str().unwrap_or("短剧")).size(18),
                        text(format!("{} 集", item["episodes"].as_u64().unwrap_or(0)))
                    ]
                    .spacing(8);
                    if let Some(cover) = item["id"].as_str().and_then(|id| page.covers.get(id)) {
                        details = details.push(
                            image(cover.clone())
                                .width(120)
                                .height(180)
                                .content_fit(iced::ContentFit::Contain),
                        );
                    }
                    list = list.push(
                        button(details)
                            .padding(12)
                            .on_press_maybe(app.can_interact().then(|| {
                                let series = item["id"].as_str().unwrap_or("").to_owned();
                                let episode =
                                    page.history[&series]["episode"].as_u64().unwrap_or(1) as u32;
                                Message::Play(series, episode)
                            }))
                            .width(Fill),
                    );
                }
                if !page.favorites_tab && page.has_more {
                    list = list.push(
                        button("加载更多")
                            .on_press_maybe(app.can_interact().then_some(Message::More)),
                    );
                }
                library
                    .push(
                        scrollable(list)
                            .height(Fill)
                            .id(iced::widget::Id::from(list_id.clone()))
                            .on_scroll(move |viewport| {
                                Message::Scroll(list_id.clone(), generation, viewport)
                            }),
                    )
                    .into()
            }
            (Some(Page::MediaLibrary { .. }), _) => container(account(app)).center(Fill).into(),
            _ => container(
                column![
                    text("你的桌面搭子").size(28),
                    text("安装需要的功能，开始摸鱼。")
                ]
                .spacing(12),
            )
            .center(Fill)
            .into(),
        },
    };
    if app.busy && app.current_method != "cover.read" {
        body = body.push(text("加载中…"));
    }
    container(body.push(content))
        .padding(20)
        .width(Fill)
        .height(Fill)
        .style(move |theme: &iced::Theme| {
            container::Style::default()
                .background(iced::Color {
                    a: app.preferences.background_opacity,
                    ..theme.palette().background
                })
                .border(iced::Border {
                    radius: 16.into(),
                    ..Default::default()
                })
        })
        .into()
}

fn card_style(theme: &iced::Theme, opacity: f32) -> container::Style {
    container::Style::default()
        .background(iced::Color {
            a: opacity,
            ..theme.extended_palette().background.weak.color
        })
        .border(iced::Border {
            radius: 12.into(),
            ..Default::default()
        })
}

fn contains(values: &Value, id: &Value) -> bool {
    values.as_array().is_some_and(|values| values.contains(id))
}

fn news_settings(app: &App) -> Element<'_, Message> {
    let Some(page) = app.page() else {
        return text("请先选择功能。").into();
    };
    if !page.news_preferences.is_object() {
        return text("正在读取新闻设置…").into();
    }
    let preferences = &page.news_preferences;
    let auto = preferences.clone();
    let interval = preferences.clone();
    let default = preferences.clone();
    let mut settings = column![
        text("新闻设置").size(24),
        checkbox(preferences["auto_refresh"] == true)
            .label("自动刷新")
            .on_toggle_maybe((!app.busy).then_some(move |value| {
                let mut preferences = auto.clone();
                preferences["auto_refresh"] = json!(value);
                Message::NewsPreferences(preferences)
            })),
        row![
            text("刷新间隔（分钟）"),
            pick_list(
                vec![1u32, 2, 5, 10, 15, 30, 60],
                preferences["refresh_minutes"]
                    .as_u64()
                    .map(|value| value as u32),
                move |value| {
                    let mut preferences = interval.clone();
                    preferences["refresh_minutes"] = json!(value);
                    Message::NewsPreferences(preferences)
                }
            )
        ]
        .spacing(12),
        checkbox(preferences["default_followed"] == true)
            .label("默认只看关注")
            .on_toggle_maybe((!app.busy).then_some(move |value| {
                let mut preferences = default.clone();
                preferences["default_followed"] = json!(value);
                Message::NewsPreferences(preferences)
            })),
        text("来源管理").size(20)
    ]
    .spacing(12);
    for source in &page.catalog {
        let id = source["id"].clone();
        let followed = contains(&preferences["followed"], &id);
        let hidden = contains(&preferences["hidden"], &id);
        let pinned = contains(&preferences["order"], &id);
        let toggle = |key: &str, present: bool| {
            let mut preferences = preferences.clone();
            let mut ids = preferences[key].as_array().cloned().unwrap_or_default();
            ids.retain(|old| old != &id);
            if !present {
                ids.push(id.clone());
            }
            preferences[key] = json!(ids);
            Message::NewsPreferences(preferences)
        };
        settings = settings.push(
            row![
                text(source["name"].as_str().unwrap_or("新闻")).width(Fill),
                button(if followed { "取消关注" } else { "关注" })
                    .on_press_maybe((!app.busy).then(|| toggle("followed", followed))),
                button(if hidden { "显示" } else { "隐藏" })
                    .on_press_maybe((!app.busy).then(|| toggle("hidden", hidden))),
                button(if pinned { "取消置顶" } else { "置顶" })
                    .on_press_maybe((!app.busy).then(|| toggle("order", pinned)))
            ]
            .spacing(8),
        );
    }
    scrollable(settings).height(Fill).into()
}

fn account(app: &App) -> Element<'_, Message> {
    let Some(page) = app.page() else {
        return text("请先选择一个功能。").into();
    };
    if page.account["state"] == "signed_in" {
        return column![
            text(page.account["name"].as_str().unwrap_or("红果用户")).size(22),
            button("退出登录").on_press_maybe(
                (!app.busy && page.account["operation"] != "pending")
                    .then_some(Message::Account("account.logout"))
            )
        ]
        .spacing(20)
        .into();
    }
    if matches!(
        page.account["state"].as_str(),
        Some("preparing" | "verifying")
    ) {
        return column![
            text(if page.account["state"] == "preparing" {
                "正在获取二维码…"
            } else {
                "正在核对身份…"
            }),
            button("取消").on_press(Message::Account("account.cancel"))
        ]
        .spacing(16)
        .into();
    }
    if matches!(page.account["state"].as_str(), Some("pending" | "scanned")) {
        let mut content = column![text("使用红果扫码登录")].spacing(16);
        if let Some(handle) = &page.login_qr {
            content = content.push(image(handle.clone()).width(220).height(220));
        }
        return content
            .push(button("取消").on_press(Message::Account("account.cancel")))
            .into();
    }
    button("登录")
        .on_press_maybe((!app.busy).then_some(Message::Account("account.login")))
        .into()
}
