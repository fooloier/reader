use crate::controls::icon;
use gpui_kit::{
    component::{
        button::Button,
        select::{Select, SelectEvent, SelectState},
        *,
    },
    prelude::*,
    *,
};
use reader_core::{Appearance, Format, PdfSettings, Settings};

pub struct SettingsPane {
    settings: Settings,
    format: Option<Format>,
    fonts: Entity<SelectState<Vec<SharedString>>>,
    _subscription: Subscription,
}
impl SettingsPane {
    pub fn new(
        settings: Settings,
        format: Option<Format>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut available = cx.text_system().all_font_names();
        available.sort();
        available.dedup();
        let mut names = vec![SharedString::from(".SystemUIFont")];
        names.extend(
            available
                .into_iter()
                .filter(|name| name != ".SystemUIFont")
                .map(SharedString::from),
        );
        let selected = names
            .iter()
            .position(|v: &SharedString| v.as_ref() == settings.epub.font)
            .map(IndexPath::new);
        let fonts = cx.new(|cx| SelectState::new(names, selected, window, cx));
        let subscription = cx.subscribe(&fonts, |this, _, event, cx| {
            if let SelectEvent::Confirm(Some(font)) = event {
                this.settings.epub.font = font.to_string();
                this.changed(cx);
            }
        });
        Self {
            settings,
            format,
            fonts,
            _subscription: subscription,
        }
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(self.settings.clone());
        cx.notify();
    }
    fn number(
        id: &'static str,
        label: &'static str,
        value: f32,
        step: f32,
        bounds: (f32, f32),
        change: fn(&mut Settings, f32),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().child(label))
            .child(
                icon(
                    SharedString::from(format!("{id}-minus")),
                    IconName::Minus,
                    format!("减小{label}"),
                )
                .disabled(value <= bounds.0)
                .on_click(cx.listener(move |this, _, _, cx| {
                    change(&mut this.settings, (value - step).max(bounds.0));
                    this.changed(cx);
                })),
            )
            .child(div().w_12().text_center().child(format!("{value:.1}")))
            .child(
                icon(
                    SharedString::from(format!("{id}-plus")),
                    IconName::Plus,
                    format!("增大{label}"),
                )
                .disabled(value >= bounds.1)
                .on_click(cx.listener(move |this, _, _, cx| {
                    change(&mut this.settings, (value + step).min(bounds.1));
                    this.changed(cx);
                })),
            )
    }
}
impl EventEmitter<Settings> for SettingsPane {}
impl Render for SettingsPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().child("外观"))
            .child(
                Button::new("settings-theme")
                    .label(if self.settings.appearance == Appearance::Light {
                        "浅色"
                    } else {
                        "深色"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings.appearance = if this.settings.appearance == Appearance::Light
                        {
                            Appearance::Dark
                        } else {
                            Appearance::Light
                        };
                        this.changed(cx);
                    })),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("阅读设置按格式应用于所有书籍"),
            )
            .when(self.format == Some(Format::Epub), |view| {
                view.child(div().child("字体"))
                    .child(Select::new(&self.fonts).id("epub-font"))
                    .child(Self::number(
                        "font-size",
                        "字号",
                        self.settings.epub.size,
                        2.,
                        (12., 40.),
                        |s, v| s.epub.size = v,
                        cx,
                    ))
                    .child(Self::number(
                        "line-height",
                        "行距",
                        self.settings.epub.line_height,
                        0.1,
                        (1., 2.5),
                        |s, v| s.epub.line_height = v,
                        cx,
                    ))
                    .child(Self::number(
                        "paragraph-gap",
                        "段距",
                        self.settings.epub.paragraph_gap,
                        0.25,
                        (0., 3.),
                        |s, v| s.epub.paragraph_gap = v,
                        cx,
                    ))
                    .child(Self::number(
                        "content-width",
                        "正文宽度",
                        self.settings.epub.width,
                        80.,
                        (320., 1600.),
                        |s, v| s.epub.width = v,
                        cx,
                    ))
            })
            .when(self.format == Some(Format::Pdf), |view| {
                view.child(div().child("PDF 缩放"))
                    .child(
                        Button::new("fit-width")
                            .label("适合宽度")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.pdf = PdfSettings::FitWidth;
                                this.changed(cx);
                            })),
                    )
                    .child(
                        Button::new("fit-page")
                            .label("适合整页")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.pdf = PdfSettings::FitPage;
                                this.changed(cx);
                            })),
                    )
                    .child(Self::number(
                        "pdf-zoom",
                        "缩放比例",
                        match self.settings.pdf {
                            PdfSettings::Zoom(v) => v,
                            _ => 1.,
                        },
                        0.25,
                        (0.25, 4.),
                        |s, v| s.pdf = PdfSettings::Zoom(v),
                        cx,
                    ))
            })
            .when(self.format.is_none(), |view| {
                view.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("打开书籍后可调整对应格式的阅读设置。"),
                )
            })
    }
}
