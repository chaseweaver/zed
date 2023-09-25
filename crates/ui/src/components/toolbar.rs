use gpui2::elements::div;
use gpui2::style::StyleHelpers;
use gpui2::{Element, IntoElement, ParentElement, ViewContext};

use crate::{theme, Breadcrumb, IconAsset, IconButton};

pub struct ToolbarItem {}

#[derive(Element)]
pub struct Toolbar {
    items: Vec<ToolbarItem>,
}

impl Toolbar {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn render<V: 'static>(&mut self, _: &mut V, cx: &mut ViewContext<V>) -> impl IntoElement<V> {
        let theme = theme(cx);

        div()
            .p_2()
            .flex()
            .justify_between()
            .child(Breadcrumb::new())
            .child(
                div()
                    .flex()
                    .child(IconButton::new(IconAsset::InlayHint))
                    .child(IconButton::new(IconAsset::MagnifyingGlass))
                    .child(IconButton::new(IconAsset::MagicWand)),
            )
    }
}