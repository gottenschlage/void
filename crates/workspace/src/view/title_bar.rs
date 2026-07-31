use std::time::Duration;

use gpui::{
    Animation, AnimationExt, Context, MouseButton, MouseDownEvent, MouseUpEvent, Role, Window,
    WindowControlArea, div, point, prelude::*, px,
};
use theme::ActiveTheme as _;
use ui::icon_sized;

use super::{branches::HEADER_HEIGHT, workspace::WorkspaceView};

pub(super) const SIDEBAR_WIDTH: f32 = 240.0;
const COLLAPSED_TITLEBAR_WIDTH: f32 = 48.0;
pub(super) const TITLEBAR_TRANSITION: Duration = Duration::from_millis(200);
const TRAFFIC_LIGHT_X: f32 = 16.0;
const TRAFFIC_LIGHT_Y: f32 = 11.0;

impl WorkspaceView {
    fn toggle_sidebar(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        self.sidebar_animation_generation = self.sidebar_animation_generation.wrapping_add(1);
        self.sync_traffic_lights(window);
        cx.stop_propagation();
        cx.notify();
    }

    fn stop_titlebar_drag(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.should_move_window = false;
        cx.stop_propagation();
    }

    fn titlebar_mouse_down(&mut self, _: &MouseDownEvent, _: &mut Window, _: &mut Context<Self>) {
        self.should_move_window = true;
    }

    fn titlebar_mouse_move(
        &mut self,
        _: &gpui::MouseMoveEvent,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        if self.should_move_window {
            self.should_move_window = false;
            window.start_window_move();
        }
    }

    pub(super) fn sync_traffic_lights(&self, window: &Window) {
        #[cfg(target_os = "macos")]
        {
            let visible = traffic_lights_visible(self.sidebar_open, window.is_fullscreen());
            if let Err(error) = super::macos_title_bar::set_traffic_lights_visible(window, visible)
            {
                eprintln!("could not update macOS traffic-light visibility: {error}");
            }
            if visible {
                window.set_traffic_light_position(point(px(TRAFFIC_LIGHT_X), px(TRAFFIC_LIGHT_Y)));
            }
        }
    }

    pub(super) fn render_titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_open = self.sidebar_open;
        let generation = self.sidebar_animation_generation;
        let has_sidebar = self.sidebar.is_some();
        let start_width = if generation == 0 {
            titlebar_leading_width(sidebar_open)
        } else if sidebar_open {
            COLLAPSED_TITLEBAR_WIDTH
        } else {
            SIDEBAR_WIDTH
        };
        let end_width = titlebar_leading_width(sidebar_open);

        div()
            .id("void-titlebar")
            .window_control_area(WindowControlArea::Drag)
            .flex()
            .h(px(HEADER_HEIGHT))
            .flex_none()
            .bg(cx.theme().colors().surface_background)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::titlebar_mouse_down))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.should_move_window = false),
            )
            .on_mouse_move(cx.listener(Self::titlebar_mouse_move))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
            .child(
                div()
                    .id(("titlebar-leading", generation))
                    .flex()
                    .flex_none()
                    .h_full()
                    .items_center()
                    .justify_end()
                    .pr_2()
                    .overflow_hidden()
                    .border_b_1()
                    .border_r_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        div()
                            .id("toggle-sidebar")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label(if sidebar_open {
                                "Close sidebar"
                            } else {
                                "Open sidebar"
                            })
                            .flex()
                            .size(px(28.))
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .when(has_sidebar, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|button| button.bg(cx.theme().colors().element_hover))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(Self::stop_titlebar_drag),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(Self::toggle_sidebar),
                                    )
                            })
                            .when(!has_sidebar, |button| button.opacity(0.35))
                            .child(icon_sized(
                                "icons/panel-left.svg",
                                16.,
                                cx.theme().colors().text_muted,
                            )),
                    )
                    .with_animation(
                        ("sidebar-titlebar-width", generation),
                        Animation::new(TITLEBAR_TRANSITION),
                        move |element, delta| {
                            element.w(px(interpolate_width(start_width, end_width, delta)))
                        },
                    ),
            )
            .when_some(self.branch_header.clone(), |titlebar, header| {
                titlebar.child(div().min_w_0().flex_1().h_full().child(header))
            })
            .when(self.branch_header.is_none(), |titlebar| {
                titlebar.child(
                    div()
                        .flex_1()
                        .h_full()
                        .border_b_1()
                        .border_color(cx.theme().colors().border_variant),
                )
            })
    }
}

pub(super) fn interpolate_width(start: f32, end: f32, delta: f32) -> f32 {
    start + (end - start) * delta.clamp(0.0, 1.0)
}

pub(super) fn default_sidebar_open() -> bool {
    true
}

fn titlebar_leading_width(sidebar_open: bool) -> f32 {
    if sidebar_open {
        SIDEBAR_WIDTH
    } else {
        COLLAPSED_TITLEBAR_WIDTH
    }
}

fn traffic_lights_visible(sidebar_open: bool, fullscreen: bool) -> bool {
    sidebar_open || fullscreen
}

#[cfg(test)]
mod tests {
    use super::{
        COLLAPSED_TITLEBAR_WIDTH, SIDEBAR_WIDTH, default_sidebar_open, interpolate_width,
        titlebar_leading_width, traffic_lights_visible,
    };

    #[test]
    fn sidebar_defaults_and_width_end_states_are_stable() {
        assert!(default_sidebar_open());
        assert_eq!(titlebar_leading_width(true), SIDEBAR_WIDTH);
        assert_eq!(titlebar_leading_width(false), COLLAPSED_TITLEBAR_WIDTH);
        assert_eq!(interpolate_width(0.0, SIDEBAR_WIDTH, 0.0), 0.0);
        assert_eq!(interpolate_width(0.0, SIDEBAR_WIDTH, 1.0), SIDEBAR_WIDTH);
        assert_eq!(interpolate_width(SIDEBAR_WIDTH, 0.0, 1.0), 0.0);
    }

    #[test]
    fn interpolation_clamps_to_reduced_motion_end_state() {
        assert_eq!(
            interpolate_width(0.0, SIDEBAR_WIDTH, f32::INFINITY),
            SIDEBAR_WIDTH
        );
    }

    #[test]
    fn traffic_lights_follow_sidebar_and_fullscreen_policy() {
        assert!(traffic_lights_visible(true, false));
        assert!(!traffic_lights_visible(false, false));
        assert!(traffic_lights_visible(false, true));
    }
}
