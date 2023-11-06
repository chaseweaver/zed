use crate::{
    self as gpui, hsla, point, px, relative, rems, AlignItems, CursorStyle, DefiniteLength,
    Display, Fill, FlexDirection, Hsla, JustifyContent, Length, Position, Rems, SharedString,
    StyleRefinement, Visibility,
};
use crate::{BoxShadow, TextStyleRefinement};
use smallvec::smallvec;

pub trait Styled {
    fn style(&mut self) -> &mut StyleRefinement;

    gpui2_macros::style_helpers!();

    /// Sets the size of the element to the full width and height.
    fn full(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().size.width = Some(relative(1.).into());
        self.style().size.height = Some(relative(1.).into());
        self
    }

    /// Sets the position of the element to `relative`.
    /// [Docs](https://tailwindcss.com/docs/position)
    fn relative(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().position = Some(Position::Relative);
        self
    }

    /// Sets the position of the element to `absolute`.
    /// [Docs](https://tailwindcss.com/docs/position)
    fn absolute(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().position = Some(Position::Absolute);
        self
    }

    /// Sets the display type of the element to `block`.
    /// [Docs](https://tailwindcss.com/docs/display)
    fn block(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().display = Some(Display::Block);
        self
    }

    /// Sets the display type of the element to `flex`.
    /// [Docs](https://tailwindcss.com/docs/display)
    fn flex(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().display = Some(Display::Flex);
        self
    }

    /// Sets the visibility of the element to `visible`.
    /// [Docs](https://tailwindcss.com/docs/visibility)
    fn visible(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().visibility = Some(Visibility::Visible);
        self
    }

    /// Sets the visibility of the element to `hidden`.
    /// [Docs](https://tailwindcss.com/docs/visibility)
    fn invisible(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().visibility = Some(Visibility::Hidden);
        self
    }

    fn cursor(mut self, cursor: CursorStyle) -> Self
    where
        Self: Sized,
    {
        self.style().mouse_cursor = Some(cursor);
        self
    }

    /// Sets the cursor style when hovering an element to `default`.
    /// [Docs](https://tailwindcss.com/docs/cursor)
    fn cursor_default(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().mouse_cursor = Some(CursorStyle::Arrow);
        self
    }

    /// Sets the cursor style when hovering an element to `pointer`.
    /// [Docs](https://tailwindcss.com/docs/cursor)
    fn cursor_pointer(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().mouse_cursor = Some(CursorStyle::PointingHand);
        self
    }

    /// Sets the flex direction of the element to `column`.
    /// [Docs](https://tailwindcss.com/docs/flex-direction#column)
    fn flex_col(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().flex_direction = Some(FlexDirection::Column);
        self
    }

    /// Sets the flex direction of the element to `row`.
    /// [Docs](https://tailwindcss.com/docs/flex-direction#row)
    fn flex_row(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().flex_direction = Some(FlexDirection::Row);
        self
    }

    /// Sets the element to allow a flex item to grow and shrink as needed, ignoring its initial size.
    /// [Docs](https://tailwindcss.com/docs/flex#flex-1)
    fn flex_1(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().flex_grow = Some(1.);
        self.style().flex_shrink = Some(1.);
        self.style().flex_basis = Some(relative(0.).into());
        self
    }

    /// Sets the element to allow a flex item to grow and shrink, taking into account its initial size.
    /// [Docs](https://tailwindcss.com/docs/flex#auto)
    fn flex_auto(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().flex_grow = Some(1.);
        self.style().flex_shrink = Some(1.);
        self.style().flex_basis = Some(Length::Auto);
        self
    }

    /// Sets the element to allow a flex item to shrink but not grow, taking into account its initial size.
    /// [Docs](https://tailwindcss.com/docs/flex#initial)
    fn flex_initial(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().flex_grow = Some(0.);
        self.style().flex_shrink = Some(1.);
        self.style().flex_basis = Some(Length::Auto);
        self
    }

    /// Sets the element to prevent a flex item from growing or shrinking.
    /// [Docs](https://tailwindcss.com/docs/flex#none)
    fn flex_none(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().flex_grow = Some(0.);
        self.style().flex_shrink = Some(0.);
        self
    }

    /// Sets the element to allow a flex item to grow to fill any available space.
    /// [Docs](https://tailwindcss.com/docs/flex-grow)
    fn grow(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().flex_grow = Some(1.);
        self
    }

    /// Sets the element to align flex items to the start of the container's cross axis.
    /// [Docs](https://tailwindcss.com/docs/align-items#start)
    fn items_start(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().align_items = Some(AlignItems::FlexStart);
        self
    }

    /// Sets the element to align flex items to the end of the container's cross axis.
    /// [Docs](https://tailwindcss.com/docs/align-items#end)
    fn items_end(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().align_items = Some(AlignItems::FlexEnd);
        self
    }

    /// Sets the element to align flex items along the center of the container's cross axis.
    /// [Docs](https://tailwindcss.com/docs/align-items#center)
    fn items_center(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().align_items = Some(AlignItems::Center);
        self
    }

    /// Sets the element to justify flex items along the container's main axis
    /// such that there is an equal amount of space between each item.
    /// [Docs](https://tailwindcss.com/docs/justify-content#space-between)
    fn justify_between(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().justify_content = Some(JustifyContent::SpaceBetween);
        self
    }

    /// Sets the element to justify flex items along the center of the container's main axis.
    /// [Docs](https://tailwindcss.com/docs/justify-content#center)
    fn justify_center(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().justify_content = Some(JustifyContent::Center);
        self
    }

    /// Sets the element to justify flex items against the start of the container's main axis.
    /// [Docs](https://tailwindcss.com/docs/justify-content#start)
    fn justify_start(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().justify_content = Some(JustifyContent::Start);
        self
    }

    /// Sets the element to justify flex items against the end of the container's main axis.
    /// [Docs](https://tailwindcss.com/docs/justify-content#end)
    fn justify_end(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().justify_content = Some(JustifyContent::End);
        self
    }

    /// Sets the element to justify items along the container's main axis such
    /// that there is an equal amount of space on each side of each item.
    /// [Docs](https://tailwindcss.com/docs/justify-content#space-around)
    fn justify_around(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().justify_content = Some(JustifyContent::SpaceAround);
        self
    }

    /// Sets the background color of the element.
    fn bg<F>(mut self, fill: F) -> Self
    where
        F: Into<Fill>,
        Self: Sized,
    {
        self.style().background = Some(fill.into());
        self
    }

    /// Sets the border color of the element.
    fn border_color<C>(mut self, border_color: C) -> Self
    where
        C: Into<Hsla>,
        Self: Sized,
    {
        self.style().border_color = Some(border_color.into());
        self
    }

    /// Sets the box shadow of the element.
    /// [Docs](https://tailwindcss.com/docs/box-shadow)
    fn shadow(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().box_shadow = Some(smallvec![
            BoxShadow {
                color: hsla(0., 0., 0., 0.1),
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
            },
            BoxShadow {
                color: hsla(0., 0., 0., 0.1),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(-1.),
            }
        ]);
        self
    }

    /// Clears the box shadow of the element.
    /// [Docs](https://tailwindcss.com/docs/box-shadow)
    fn shadow_none(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().box_shadow = Some(Default::default());
        self
    }

    /// Sets the box shadow of the element.
    /// [Docs](https://tailwindcss.com/docs/box-shadow)
    fn shadow_sm(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().box_shadow = Some(smallvec::smallvec![BoxShadow {
            color: hsla(0., 0., 0., 0.05),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
        }]);
        self
    }

    /// Sets the box shadow of the element.
    /// [Docs](https://tailwindcss.com/docs/box-shadow)
    fn shadow_md(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().box_shadow = Some(smallvec![
            BoxShadow {
                color: hsla(0.5, 0., 0., 0.1),
                offset: point(px(0.), px(4.)),
                blur_radius: px(6.),
                spread_radius: px(-1.),
            },
            BoxShadow {
                color: hsla(0., 0., 0., 0.1),
                offset: point(px(0.), px(2.)),
                blur_radius: px(4.),
                spread_radius: px(-2.),
            }
        ]);
        self
    }

    /// Sets the box shadow of the element.
    /// [Docs](https://tailwindcss.com/docs/box-shadow)
    fn shadow_lg(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().box_shadow = Some(smallvec![
            BoxShadow {
                color: hsla(0., 0., 0., 0.1),
                offset: point(px(0.), px(10.)),
                blur_radius: px(15.),
                spread_radius: px(-3.),
            },
            BoxShadow {
                color: hsla(0., 0., 0., 0.1),
                offset: point(px(0.), px(4.)),
                blur_radius: px(6.),
                spread_radius: px(-4.),
            }
        ]);
        self
    }

    /// Sets the box shadow of the element.
    /// [Docs](https://tailwindcss.com/docs/box-shadow)
    fn shadow_xl(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().box_shadow = Some(smallvec![
            BoxShadow {
                color: hsla(0., 0., 0., 0.1),
                offset: point(px(0.), px(20.)),
                blur_radius: px(25.),
                spread_radius: px(-5.),
            },
            BoxShadow {
                color: hsla(0., 0., 0., 0.1),
                offset: point(px(0.), px(8.)),
                blur_radius: px(10.),
                spread_radius: px(-6.),
            }
        ]);
        self
    }

    /// Sets the box shadow of the element.
    /// [Docs](https://tailwindcss.com/docs/box-shadow)
    fn shadow_2xl(mut self) -> Self
    where
        Self: Sized,
    {
        self.style().box_shadow = Some(smallvec![BoxShadow {
            color: hsla(0., 0., 0., 0.25),
            offset: point(px(0.), px(25.)),
            blur_radius: px(50.),
            spread_radius: px(-12.),
        }]);
        self
    }

    fn text_style(&mut self) -> &mut Option<TextStyleRefinement> {
        let style: &mut StyleRefinement = self.style();
        &mut style.text
    }

    fn text_color(mut self, color: impl Into<Hsla>) -> Self
    where
        Self: Sized,
    {
        self.text_style().get_or_insert_with(Default::default).color = Some(color.into());
        self
    }

    fn text_size(mut self, size: impl Into<Rems>) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_size = Some(size.into());
        self
    }

    fn text_xs(mut self) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_size = Some(rems(0.75));
        self
    }

    fn text_sm(mut self) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_size = Some(rems(0.875));
        self
    }

    fn text_base(mut self) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_size = Some(rems(1.0));
        self
    }

    fn text_lg(mut self) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_size = Some(rems(1.125));
        self
    }

    fn text_xl(mut self) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_size = Some(rems(1.25));
        self
    }

    fn text_2xl(mut self) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_size = Some(rems(1.5));
        self
    }

    fn text_3xl(mut self) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_size = Some(rems(1.875));
        self
    }

    fn text_decoration_none(mut self) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .underline = None;
        self
    }

    fn text_decoration_color(mut self, color: impl Into<Hsla>) -> Self
    where
        Self: Sized,
    {
        let style = self.text_style().get_or_insert_with(Default::default);
        let underline = style.underline.get_or_insert_with(Default::default);
        underline.color = Some(color.into());
        self
    }

    fn text_decoration_solid(mut self) -> Self
    where
        Self: Sized,
    {
        let style = self.text_style().get_or_insert_with(Default::default);
        let underline = style.underline.get_or_insert_with(Default::default);
        underline.wavy = false;
        self
    }

    fn text_decoration_wavy(mut self) -> Self
    where
        Self: Sized,
    {
        let style = self.text_style().get_or_insert_with(Default::default);
        let underline = style.underline.get_or_insert_with(Default::default);
        underline.wavy = true;
        self
    }

    fn text_decoration_0(mut self) -> Self
    where
        Self: Sized,
    {
        let style = self.text_style().get_or_insert_with(Default::default);
        let underline = style.underline.get_or_insert_with(Default::default);
        underline.thickness = px(0.);
        self
    }

    fn text_decoration_1(mut self) -> Self
    where
        Self: Sized,
    {
        let style = self.text_style().get_or_insert_with(Default::default);
        let underline = style.underline.get_or_insert_with(Default::default);
        underline.thickness = px(1.);
        self
    }

    fn text_decoration_2(mut self) -> Self
    where
        Self: Sized,
    {
        let style = self.text_style().get_or_insert_with(Default::default);
        let underline = style.underline.get_or_insert_with(Default::default);
        underline.thickness = px(2.);
        self
    }

    fn text_decoration_4(mut self) -> Self
    where
        Self: Sized,
    {
        let style = self.text_style().get_or_insert_with(Default::default);
        let underline = style.underline.get_or_insert_with(Default::default);
        underline.thickness = px(4.);
        self
    }

    fn text_decoration_8(mut self) -> Self
    where
        Self: Sized,
    {
        let style = self.text_style().get_or_insert_with(Default::default);
        let underline = style.underline.get_or_insert_with(Default::default);
        underline.thickness = px(8.);
        self
    }

    fn font(mut self, family_name: impl Into<SharedString>) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_family = Some(family_name.into());
        self
    }

    fn line_height(mut self, line_height: impl Into<DefiniteLength>) -> Self
    where
        Self: Sized,
    {
        self.text_style()
            .get_or_insert_with(Default::default)
            .line_height = Some(line_height.into());
        self
    }
}
