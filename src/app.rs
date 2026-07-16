use cosmic::app::Core;
use cosmic::iced::core::animation::{Animation, Easing};
use cosmic::iced::mouse;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Length, Limits, Subscription};
use cosmic::widget::{button, canvas, column, container, divider, icon, row, text};
use cosmic::{Action, Element, Task, Theme};
use cosmic::iced::Color;
use std::time::{Duration, Instant};

use crate::battery;

const ID: &str = "io.github.AceMythos.cosmic-ext-applet-power-monitor";

pub struct PowerMonitor {
    core: Core,
    popup: Option<Id>,
    watts: f64,
    display_watts: Animation<f32>,
    percentage: f64,
    status: String,
    time_to_empty: i64,
    time_to_full: i64,
    energy: f64,
    energy_full: f64,
    no_battery: bool,
}

impl Default for PowerMonitor {
    fn default() -> Self {
        Self {
            core: Core::default(),
            popup: None,
            watts: 0.0,
            display_watts: Animation::new(0.0)
                .duration(Duration::from_millis(200))
                .easing(Easing::EaseOutCubic),
            percentage: 0.0,
            status: String::new(),
            time_to_empty: 0,
            time_to_full: 0,
            energy: 0.0,
            energy_full: 0.0,
            no_battery: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Update(battery::BatteryData),
    NoBattery,
}

impl PowerMonitor {
    fn format_watts(w: f64) -> String {
        if w >= 10.0 {
            format!("{:.1}W", w)
        } else if w >= 1.0 {
            format!("{:.1}W", w)
        } else if w >= 0.1 {
            format!("{:.2}W", w)
        } else {
            format!("{:.3}W", w)
        }
    }

    fn format_power_string(&self, watts: f64) -> String {
        if self.no_battery || watts <= 0.0 {
            return String::new();
        }
        let sign = if self.status == "Charging" { "+" } else { "-" };
        let time = match self.status.as_str() {
            "Charging" if self.time_to_full > 0 => format!("({})", Self::format_time(self.time_to_full)),
            "Discharging" if self.time_to_empty > 0 => format!("({})", Self::format_time(self.time_to_empty)),
            _ => String::new(),
        };
        format!("{}{}{}", sign, Self::format_watts(watts), time)
    }

    fn format_time(seconds: i64) -> String {
        if seconds <= 0 {
            return String::new();
        }
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else if minutes > 0 {
            format!("{}m", minutes)
        } else {
            format!("{}s", seconds)
        }
    }
}

struct BatteryBar {
    percentage: f32,
}

impl canvas::Program<Message, cosmic::Theme> for BatteryBar {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &cosmic::iced::Renderer,
        _theme: &Theme,
        bounds: cosmic::iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let width = bounds.width;
        let height = bounds.height;
        let fill_width = width * self.percentage;

        let track_color = Color::from_rgba(0.5, 0.5, 0.5, 0.15);
        frame.fill_rectangle(
            cosmic::iced::Point::new(0.0, 0.0),
            cosmic::iced::Size::new(width, height),
            track_color,
        );

        let fill_color = if self.percentage > 0.6 {
            Color::from_rgb(0.3, 0.8, 0.3)
        } else if self.percentage > 0.2 {
            Color::from_rgb(0.9, 0.6, 0.1)
        } else {
            Color::from_rgb(0.8, 0.2, 0.2)
        };

        if fill_width > 0.0 {
            frame.fill_rectangle(
                cosmic::iced::Point::new(0.0, 0.0),
                cosmic::iced::Size::new(fill_width, height),
                fill_color,
            );
        }

        vec![frame.into_geometry()]
    }
}

impl cosmic::Application for PowerMonitor {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let app = PowerMonitor {
            core,
            ..Default::default()
        };
        (
            app,
            Task::perform(battery::poll_battery(), |result| match result {
                Ok(data) => Message::Update(data),
                Err(_) => Message::NoBattery,
            })
            .map(Action::App),
        )
    }

    fn on_close_requested(&self, id: cosmic::iced::window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(popup_id) = self.popup.take() {
                    destroy_popup(popup_id)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);

                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(popup_id) => {
                if self.popup.as_ref() == Some(&popup_id) {
                    self.popup = None;
                }
            }
            Message::Update(data) => {
                self.watts = data.energy_rate;
                self.display_watts.go_mut(data.energy_rate as f32, Instant::now());
                self.percentage = data.percentage;
                self.status = data.status;
                self.time_to_empty = data.time_to_empty;
                self.time_to_full = data.time_to_full;
                self.energy = data.energy;
                self.energy_full = data.energy_full;
                self.no_battery = false;
            }
            Message::NoBattery => {
                self.no_battery = true;
                self.watts = 0.0;
                self.percentage = 0.0;
                self.status = String::new();
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let animated = self.display_watts.interpolate_with(|v| v, Instant::now()) as f64;
        let content = text::body(self.format_power_string(animated));

        let btn = button::custom(content)
            .on_press_down(Message::TogglePopup)
            .padding([4, 8]);

        self.core.applet.autosize_window(btn).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content: Vec<Element<Message>> = Vec::new();

        if self.no_battery {
            content.push(
                container(text::body("No battery detected")).padding(12).into(),
            );
            return self.core.applet.popup_container(column::with_children(content)).into();
        }

        let status_icon = match self.status.as_str() {
            "Charging" => "emblem-ok-symbolic",
            "Discharging" => "battery-level-50-symbolic",
            "Fully Charged" => "battery-level-100-symbolic",
            _ => "battery-symbolic",
        };

        content.push(
            container(
                row![
                    icon::from_name(status_icon).size(32),
                    column![
                        text::title1(format!("{:.0}%", self.percentage)),
                        text::caption(&self.status),
                    ]
                    .spacing(0),
                ]
                .spacing(12)
                .align_y(cosmic::iced::core::Alignment::Center),
            )
            .padding([12, 12, 4, 12])
            .into(),
        );

        content.push(
            container(
                canvas::Canvas::<BatteryBar, Message, Theme>::new(BatteryBar {
                    percentage: (self.percentage / 100.0) as f32,
                })
                .width(Length::Fill)
                .height(Length::Fixed(4.0)),
            )
            .padding([4, 12, 12, 12])
            .into(),
        );

        content.push(divider::horizontal::default().into());

        if self.watts > 0.0 {
            let label = if self.status == "Charging" {
                "Charge rate"
            } else {
                "Discharge rate"
            };
            content.push(
                container(
                    row![
                        text::body(label).width(cosmic::iced::Length::Fill),
                        text::body(Self::format_watts(self.watts)),
                    ]
                    .align_y(cosmic::iced::core::Alignment::Center),
                )
                .padding([6, 12])
                .into(),
            );
        }

        content.push(
            container(
                row![
                    text::body("Energy remaining").width(cosmic::iced::Length::Fill),
                    text::body(format!("{:.1} Wh", self.energy)),
                ]
                .align_y(cosmic::iced::core::Alignment::Center),
            )
            .padding([6, 12])
            .into(),
        );

        content.push(
            container(
                row![
                    text::body("Full capacity").width(cosmic::iced::Length::Fill),
                    text::body(format!("{:.1} Wh", self.energy_full)),
                ]
                .align_y(cosmic::iced::core::Alignment::Center),
            )
            .padding([6, 12])
            .into(),
        );

        if self.status == "Discharging" && self.time_to_empty > 0 {
            content.push(
                container(
                    row![
                        text::body("Time to empty").width(cosmic::iced::Length::Fill),
                        text::body(Self::format_time(self.time_to_empty)),
                    ]
                    .align_y(cosmic::iced::core::Alignment::Center),
                )
                .padding([6, 12])
                .into(),
            );
        }

        if self.status == "Charging" && self.time_to_full > 0 {
            content.push(
                container(
                    row![
                        text::body("Time to full").width(cosmic::iced::Length::Fill),
                        text::body(Self::format_time(self.time_to_full)),
                    ]
                    .align_y(cosmic::iced::core::Alignment::Center),
                )
                .padding([6, 12])
                .into(),
            );
        }

        self.core.applet.popup_container(column::with_children(content)).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(
            std::any::TypeId::of::<()>(),
            |_state| {
                futures_util::stream::unfold(
                    (),
                    |_| async move {
                        let message = match battery::poll_battery().await {
                            Ok(data) => Some((Message::Update(data), ())),
                            Err(_) => Some((Message::NoBattery, ())),
                        };
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        message
                    },
                )
            },
        )
    }
}
