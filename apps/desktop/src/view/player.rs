use super::*;
use iced::widget::{column, row};
pub(super) fn view<'a>(app: &'a App, page: &'a PageState) -> Element<'a, Message> {
    let series = page.media["series"].as_str().unwrap_or("").to_owned();
    let episode = page.media["episode"].as_u64().unwrap_or(1) as u32;
    let episodes: Vec<u32> = page.media["episodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_u64().and_then(|value| u32::try_from(value).ok()))
        .collect();
    let playing = page.media["playing"] == true;
    let favorite = page
        .favorite_override
        .unwrap_or_else(|| page.favorites.iter().any(|row| row["id"] == series));
    let preferences = app
        .preferences
        .pages
        .get(&app.selected.as_ref().unwrap().id)
        .cloned()
        .unwrap_or_default();
    let qualities: Vec<String> = page.media["available_qualities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .map(|height| format!("{height}p"))
        .collect();
    let previous = episodes
        .iter()
        .copied()
        .filter(|number| *number < episode)
        .next_back();
    let next = episodes.iter().copied().find(|number| *number > episode);
    let previous_series = series.clone();
    let next_series = series.clone();
    let frame: Element<'_, Message> = if let Some(frame) = &app.frame {
        iced::widget::shader::Shader::new(lazybuddy_media_ipc::view::Video {
            frame: frame.clone(),
            opacity: app.preferences.card_opacity,
        })
        .width(Fill)
        .height(Fill)
        .into()
    } else {
        container(text(if page.media["loading"] == true {
            "正在载入视频…"
        } else {
            "点击播放继续观看"
        }))
        .center(Fill)
        .into()
    };
    let duration = page.media["duration"].as_f64().unwrap_or(0.).max(0.1);
    let position = page
        .seek_preview
        .unwrap_or_else(|| page.media["position"].as_f64().unwrap_or(0.));
    let mut layers = iced::widget::stack![frame];
    if page.controls_hover
        || page.controls_keyboard
        || page.controls_menu
        || page.media["loaded"] != true
    {
        let title = page
            .favorites
            .iter()
            .chain(page.cards.iter())
            .find(|row| row["id"] == series)
            .and_then(|row| row["title"].as_str())
            .unwrap_or("短剧");
        let top = row![
            media_icon(
                include_bytes!(
                    "../../../../assets/button-icons/ic_fluent_arrow_left_20_filled.png"
                ),
                Some(Message::Player("media.close", json!({}))),
                false
            ),
            text(format!("{title}  第 {episode} 集")).width(Fill),
            media_icon(
                if favorite {
                    include_bytes!("../../../../assets/button-icons/ic_fluent_star_20_filled.png")
                } else {
                    include_bytes!("../../../../assets/button-icons/ic_fluent_star_20_regular.png")
                },
                (app.can_interact()
                    && page.favorite_override.is_none()
                    && page.account["operation"] != "pending")
                    .then_some(Message::Favorite(!favorite)),
                favorite
            )
        ]
        .spacing(8)
        .align_y(iced::Center);
        let mut top = column![top].spacing(4);
        if let Some(error) = page.media["progress_error"].as_str() {
            top = top.push(
                text(if error == "progress_save_failed" {
                    "观看进度保存失败，原文件已保留。"
                } else {
                    "云端进度尚未同步，本地待上传记录已保留。"
                })
                .size(12),
            );
        }
        let mut transport = row![
            media_icon(
                include_bytes!("../../../../assets/button-icons/ic_fluent_previous_20_filled.png"),
                previous
                    .filter(|_| app.can_interact())
                    .map(|episode| Message::Play(previous_series, episode)),
                false
            ),
            media_icon(
                if playing {
                    include_bytes!("../../../../assets/button-icons/ic_fluent_pause_20_filled.png")
                } else {
                    include_bytes!("../../../../assets/button-icons/ic_fluent_play_20_filled.png")
                },
                app.can_interact().then_some(Message::Player(
                    if playing { "media.pause" } else { "media.play" },
                    json!({})
                )),
                false
            ),
            media_icon(
                include_bytes!("../../../../assets/button-icons/ic_fluent_next_20_filled.png"),
                next.filter(|_| app.can_interact())
                    .map(|episode| Message::Play(next_series, episode)),
                false
            ),
            text(format!("{} / {}", clock(position), clock(duration))).size(13),
        ]
        .spacing(4)
        .align_y(iced::Center);
        if let (Some(width), Some(height)) = (
            page.media["decoded_width"].as_u64(),
            page.media["decoded_height"].as_u64(),
        ) {
            transport = transport.push(text(format!("{width}×{height}")).size(12));
        }
        let options = row![
            pick_list(episodes, Some(episode), move |episode| Message::Play(
                series.clone(),
                episode
            ))
            .handle(iced::widget::pick_list::Handle::None)
            .text_alignment(iced::alignment::Horizontal::Center)
            .on_open(Message::ControlsMenu(true))
            .on_close(Message::ControlsMenu(false))
            .style(media_pick),
            pick_list(qualities, Some(preferences.quality), Message::Quality)
                .handle(iced::widget::pick_list::Handle::None)
                .text_alignment(iced::alignment::Horizontal::Center)
                .on_open(Message::ControlsMenu(true))
                .on_close(Message::ControlsMenu(false))
                .style(media_pick),
            pick_list(
                vec![0.5_f64, 0.75, 1., 1.25, 1.5, 2., 3., 4.],
                Some(preferences.speed),
                |value| Message::Player("media.speed", json!({"value":value}))
            )
            .handle(iced::widget::pick_list::Handle::None)
            .text_alignment(iced::alignment::Horizontal::Center)
            .on_open(Message::ControlsMenu(true))
            .on_close(Message::ControlsMenu(false))
            .style(media_pick),
            media_icon(
                if preferences.mute {
                    include_bytes!(
                        "../../../../assets/button-icons/ic_fluent_speaker_mute_20_filled.png"
                    )
                } else {
                    include_bytes!(
                        "../../../../assets/button-icons/ic_fluent_speaker_2_20_filled.png"
                    )
                },
                app.can_interact().then_some(Message::Player(
                    "media.mute",
                    json!({"value":!preferences.mute})
                )),
                false
            ),
            iced::widget::slider(0.0..=1.0, preferences.volume, Message::VolumePreview)
                .step(0.01_f64)
                .on_release(Message::VolumeCommit)
                .width(70)
        ]
        .spacing(4)
        .align_y(iced::Center);
        let controls: Element<'_, Message> = if app.width < 760. {
            column![transport, options].spacing(4).into()
        } else {
            row![transport, iced::widget::space().width(Fill), options]
                .align_y(iced::Center)
                .into()
        };
        let bottom = column![
            iced::widget::slider(0.0..=duration, position, Message::SeekPreview)
                .height(12)
                .on_release(Message::SeekCommit),
            controls
        ]
        .spacing(4);
        layers = layers.push(
            column![
                container(top)
                    .padding(10)
                    .width(Fill)
                    .style(|_| media_bar(true)),
                iced::widget::space().height(Fill),
                container(bottom)
                    .padding(10)
                    .width(Fill)
                    .style(|_| media_bar(false))
            ]
            .height(Fill),
        );
    }
    iced::widget::mouse_area(layers.width(Fill).height(Fill))
        .on_enter(Message::ControlsHover(true))
        .on_exit(Message::ControlsHover(false))
        .into()
}

fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
fn media_bar(top: bool) -> container::Style {
    let dark = iced::Color::from_rgba(0., 0., 0., 0.85);
    container::Style::default()
        .color(iced::Color::WHITE)
        .background(
            iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                .add_stop(0., if top { dark } else { iced::Color::TRANSPARENT })
                .add_stop(1., if top { iced::Color::TRANSPARENT } else { dark }),
        )
}
fn media_pick(
    _: &iced::Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    let mut style = iced::widget::pick_list::default(&iced::Theme::Dark, status);
    style.background = iced::Color::TRANSPARENT.into();
    style.text_color = iced::Color::WHITE;
    style.border = iced::Border::default();
    style
}
fn media_icon(
    bytes: &'static [u8],
    message: Option<Message>,
    gold: bool,
) -> iced::widget::Button<'static, Message> {
    thread_local! {static ICONS:std::cell::RefCell<std::collections::BTreeMap<(usize,bool),iced::widget::image::Handle>>=Default::default();}
    let handle = ICONS.with(|icons| {
        icons
            .borrow_mut()
            .entry((bytes.as_ptr() as usize, gold))
            .or_insert_with(|| {
                let mut pixels = ::image::load_from_memory(bytes)
                    .expect("bundled media icon")
                    .to_rgba8();
                if gold {
                    for pixel in pixels.pixels_mut() {
                        pixel[0] = 255;
                        pixel[1] = 204;
                        pixel[2] = 0;
                    }
                }
                iced::widget::image::Handle::from_rgba(
                    pixels.width(),
                    pixels.height(),
                    pixels.into_raw(),
                )
            })
            .clone()
    });
    button(image(handle).width(20).height(20))
        .padding(6)
        .on_press_maybe(message)
        .style(|theme, status| {
            let mut style = iced::widget::button::text(theme, status);
            style.text_color = iced::Color::WHITE;
            style
        })
}
