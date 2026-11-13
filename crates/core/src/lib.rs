use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BookId(pub i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Pdf,
    Epub,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Epub => "epub",
        }
    }
    pub fn from_extension(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "pdf" => Some(Self::Pdf),
            "epub" => Some(Self::Epub),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum Location {
    Pdf { page: usize, fraction: f32 },
    Epub { href: String, anchor: String },
}

impl Location {
    pub fn matches(&self, format: Format) -> bool {
        matches!((self, format), (Self::Pdf { fraction, .. }, Format::Pdf) if fraction.is_finite() && (0.0..=1.0).contains(fraction))
            || matches!((self, format), (Self::Epub { .. }, Format::Epub))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub id: BookId,
    pub format: Format,
    pub metadata: Metadata,
    pub display_title: String,
    pub filename: String,
    pub digest: String,
    pub cover: Option<String>,
    pub location: Option<Location>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShelfLayout {
    #[default]
    Cards,
    List,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpubSettings {
    pub font: String,
    pub size: f32,
    pub line_height: f32,
    pub paragraph_gap: f32,
    pub width: f32,
}
impl Default for EpubSettings {
    fn default() -> Self {
        Self {
            font: ".SystemUIFont".into(),
            size: 20.,
            line_height: 1.7,
            paragraph_gap: 1.,
            width: 720.,
        }
    }
}
impl EpubSettings {
    pub fn is_valid(&self) -> bool {
        !self.font.trim().is_empty()
            && self.font.len() <= 256
            && (12.0..=40.0).contains(&self.size)
            && (1.0..=2.5).contains(&self.line_height)
            && (0.0..=3.0).contains(&self.paragraph_gap)
            && (320.0..=1600.0).contains(&self.width)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
pub enum PdfSettings {
    #[default]
    FitWidth,
    FitPage,
    Zoom(f32),
}
impl PdfSettings {
    pub fn is_valid(self) -> bool {
        match self {
            Self::Zoom(z) => (0.25..=4.0).contains(&z),
            _ => true,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub appearance: Appearance,
    pub shelf: ShelfLayout,
    pub epub: EpubSettings,
    pub pdf: PdfSettings,
}
impl Settings {
    pub fn is_valid(&self) -> bool {
        self.epub.is_valid() && self.pdf.is_valid()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub tabs: Vec<BookId>,
    pub active: Option<BookId>,
}
impl Workspace {
    pub fn open(&mut self, id: BookId) {
        if !self.tabs.contains(&id) {
            self.tabs.push(id);
        }
        self.active = Some(id);
    }
    pub fn close(&mut self, id: BookId) {
        self.tabs.retain(|v| *v != id);
        if self.active == Some(id) {
            self.active = None;
        }
    }
    pub fn retain(&mut self, valid: impl Fn(BookId) -> bool) {
        let mut seen = Vec::new();
        self.tabs.retain(|id| {
            valid(*id)
                && if seen.contains(id) {
                    false
                } else {
                    seen.push(*id);
                    true
                }
        });
        if self.active.is_some_and(|id| !self.tabs.contains(&id)) {
            self.active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tab_identity_and_home_are_stable() {
        let mut w = Workspace::default();
        w.open(BookId(3));
        w.open(BookId(3));
        assert_eq!(w.tabs.len(), 1);
        w.close(BookId(3));
        assert_eq!(w.active, None);
    }
    #[test]
    fn location_and_settings_reject_invalid_values() {
        assert!(
            !Location::Pdf {
                page: 2,
                fraction: f32::NAN
            }
            .matches(Format::Pdf)
        );
        assert!(
            !Location::Epub {
                href: "a".into(),
                anchor: "b".into()
            }
            .matches(Format::Pdf)
        );
        assert!(Settings::default().is_valid());
        assert!(!PdfSettings::Zoom(0.).is_valid());
    }
}
