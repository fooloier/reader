use gpui_kit::{
    ElementId,
    component::{IconName, Sizable, button::Button, button::ButtonVariants},
};

pub fn icon(
    id: impl Into<ElementId>,
    name: IconName,
    label: impl Into<gpui_kit::SharedString>,
) -> Button {
    let label = label.into();
    Button::new(id)
        .ghost()
        .small()
        .icon(name)
        .tooltip(label.clone())
        .accessibility_label(label)
}

pub fn observed(element: gpui_kit::Stateful<gpui_kit::Div>) -> gpui_kit::AnyElement {
    use gpui_kit::IntoElement;
    #[cfg(feature = "test-support")]
    {
        use gpui_kit::test::TestSupportExt;
        element.test_support().into_any_element()
    }
    #[cfg(not(feature = "test-support"))]
    element.into_any_element()
}
