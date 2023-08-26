use crate::{
    element::{AnyElement, Element, IntoElement, Layout, ParentElement},
    interactive::{InteractionHandlers, Interactive},
    layout_context::LayoutContext,
    paint_context::PaintContext,
    style::{Style, StyleHelpers, Styleable},
};
use anyhow::Result;
use gpui::{LayoutId, RenderContext};
use refineable::{Refineable, RefinementCascade};
use smallvec::SmallVec;

pub struct Div<V: 'static> {
    styles: RefinementCascade<Style>,
    handlers: InteractionHandlers<V>,
    children: SmallVec<[AnyElement<V>; 2]>,
}

pub fn div<V>() -> Div<V> {
    Div {
        styles: Default::default(),
        handlers: Default::default(),
        children: Default::default(),
    }
}

impl<V: 'static> Element<V> for Div<V> {
    type Layout = ();

    fn layout(&mut self, view: &mut V, cx: &mut LayoutContext<V>) -> Result<Layout<V, ()>>
    where
        Self: Sized,
    {
        let style = self.computed_style();
        let pop_text_style = style.text_style().map_or(false, |style| {
            cx.push_text_style(cx.text_style().clone().refined(&style));
            true
        });

        let children = self
            .children
            .iter_mut()
            .map(|child| child.layout(view, cx))
            .collect::<Result<Vec<LayoutId>>>()?;

        if pop_text_style {
            cx.pop_text_style();
        }

        let layout = cx.add_layout_node(style, (), children.clone())?;

        dbg!(layout.id(), children);
        Ok(layout)
    }

    fn paint(&mut self, view: &mut V, layout: &mut Layout<V, ()>, cx: &mut PaintContext<V>)
    where
        Self: Sized,
    {
        let style = &self.computed_style();
        let pop_text_style = style.text_style().map_or(false, |style| {
            cx.push_text_style(cx.text_style().clone().refined(&style));
            true
        });
        style.paint_background(layout.bounds(cx), cx);
        self.interaction_handlers()
            .paint(layout.order(cx), layout.bounds(cx), cx);
        for child in &mut self.children {
            child.paint(view, cx);
        }
        if pop_text_style {
            cx.pop_text_style();
        }
    }
}

impl<V> Styleable for Div<V> {
    type Style = Style;

    fn style_cascade(&mut self) -> &mut RefinementCascade<Self::Style> {
        &mut self.styles
    }

    fn declared_style(&mut self) -> &mut <Self::Style as Refineable>::Refinement {
        self.styles.base()
    }
}

impl<V> StyleHelpers for Div<V> {}

impl<V> Interactive<V> for Div<V> {
    fn interaction_handlers(&mut self) -> &mut InteractionHandlers<V> {
        &mut self.handlers
    }
}

impl<V: 'static> ParentElement<V> for Div<V> {
    fn children_mut(&mut self) -> &mut SmallVec<[AnyElement<V>; 2]> {
        &mut self.children
    }
}

impl<V: 'static> IntoElement<V> for Div<V> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}