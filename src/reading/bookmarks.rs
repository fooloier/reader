use super::{ReaderState, ReadingSession};
use gpui_kit::App;

pub(crate) struct Bookmark {
    pub index: usize,
    pub title: String,
    pub depth: usize,
    pub destination: BookmarkDestination,
}

pub(crate) enum BookmarkDestination {
    Pdf(usize),
    Epub {
        href: String,
        position: (usize, usize),
    },
}

impl BookmarkDestination {
    fn position(&self) -> (usize, usize) {
        match self {
            Self::Pdf(page) => (*page, 0),
            Self::Epub { position, .. } => *position,
        }
    }
}

impl ReadingSession {
    pub(crate) fn bookmarks(&self, cx: &App) -> (Vec<Bookmark>, Option<usize>) {
        let (entries, current): (Vec<Bookmark>, _) = match &self.state {
            ReaderState::Pdf(pdf) => {
                let pdf = pdf.read(cx);
                let entries = pdf
                    .outline()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| {
                        let page = item.page.filter(|page| *page < pdf.page_count())?;
                        Some(Bookmark {
                            index,
                            title: item.title.clone(),
                            depth: item.level,
                            destination: BookmarkDestination::Pdf(page),
                        })
                    })
                    .collect();
                (entries, (pdf.viewport().0, 0))
            }
            ReaderState::Epub(pane) => {
                let pane = pane.read(cx);
                let entries = pane
                    .document
                    .toc
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| {
                        let position = pane.document.destination(&item.href)?;
                        Some(Bookmark {
                            index,
                            title: item.title.clone(),
                            depth: item.depth,
                            destination: BookmarkDestination::Epub {
                                href: item.href.clone(),
                                position,
                            },
                        })
                    })
                    .collect();
                (entries, pane.reading_position())
            }
            _ => return (vec![], None),
        };
        let active = active_bookmark(entries.iter().map(|b| b.destination.position()), current);
        (entries, active)
    }
}

fn active_bookmark(
    destinations: impl Iterator<Item = (usize, usize)>,
    current: (usize, usize),
) -> Option<usize> {
    destinations
        .enumerate()
        .filter(|(_, destination)| *destination <= current)
        .max_by_key(|(_, destination)| *destination)
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::active_bookmark;

    #[test]
    fn current_bookmark_uses_document_order_and_in_chapter_position() {
        let destinations = [(2, 0), (0, 10), (0, 30), (1, 0)];
        for (current, expected) in [
            ((0, 0), None),
            ((0, 10), Some(1)),
            ((0, 29), Some(1)),
            ((0, 30), Some(2)),
            ((1, 20), Some(3)),
            ((3, 0), Some(0)),
        ] {
            assert_eq!(active_bookmark(destinations.into_iter(), current), expected);
        }
    }

    #[test]
    fn duplicate_destinations_select_one_entry_and_empty_bookmarks_select_none() {
        assert_eq!(
            active_bookmark([(0, 0), (0, 0)].into_iter(), (0, 0)),
            Some(1)
        );
        assert_eq!(active_bookmark(std::iter::empty(), (0, 0)), None);
    }
}
