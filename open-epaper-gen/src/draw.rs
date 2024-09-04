//! This module contains primitives for drawing an image for an ePaper tag as
//! well as a basic layout system roughly based on what SwiftUI's doing.
//!
//! Everything in this module is drawn on a [Surface]. The surface itself is
//! just a very thin wrapper around an [image::RgbImage] -- in fact, all of the
//! other drawing primitives in here access `surface.img`, which directly
//! exposes the [image::RgbImage].
//!
//! Everything else -- all the drawing primitives and layout helpers --
//! implements the [View] trait. The most important thing about the [View] is
//! the method `draw(&self, surface: &mut Surface, x: u32, y: u32, suggested_bounds: Bounds)`.
//! If you want to draw a primitive or layout helper onto a surface, that's the
//! method to use. Most of the time, you'll want to only call that method on a
//! top-level layout helper, though.
//!
//! As of now, the only two drawing primitives supported are [Text] and [Image].
//! Both are fairly straightforward to use, see their respective documentation.
//!
//! ## Layout
//!
//! The two main layout helpers are [HStack] and [VStack]. The first stacks
//! views horizontally, the second stacks them vertically. Using those two in
//! combination with [Spacer] allows building layouts quite easily, without
//! having to worry about coordinates for individual UI elements.
//!
//! A simple combination of [HStack] and [VStack] would allow you to build a
//! layout like this (all views are text views).
//!
//! ```none
//! +------------+------------+
//! | +--------+ | +--------+ |
//! | | Berlin | | | London | |
//! | +--------+ | +--------+ | (empty space here)
//! | | 14:21  | | | 13:21  | |
//! | +--------+ | +--------+ |
//! +------------+------------+
//! ```
//!
//! The code for this might look something like this:
//!
//! ```
//! let mut screen = HStack::new();
//!
//! let mut berlin = VStack::new();
//! berlin.views.push(Box::new(Text::new(String::from("Berlin"), 13.0, 0)));
//! berlin.views.push(Box::new(Text::new(String::from("14:21"), 13.0, 0)));
//!
//! let mut london = VStack::new();
//! london.views.push(Box::new(Text::new(String::from("London"), 13.0, 0)));
//! london.views.push(Box::new(Text::new(String::from("13:21"), 13.0, 0)));
//!
//! screen.views.push(berlin);
//! screen.views.push(london);
//! ```
//!
//! Let's say our screen is a bit wider than in the example above, and we want
//! to push Berlin to the left of the screen and London to the right of the
//! screen, instead of having it all on the left hand side of the screen. We can
//! achieve that using a [Spacer], which will use up all the available space
//! in a stack that's not required by the other views. Using [Padding] -- which
//! is supported by all [View]s -- we can add some extra spacing on the left and
//! right edge.
//!
//! ```none
//! +--------------+------------+--------------+
//! |   +--------+ |            | +--------+   |
//! |   | Berlin | |            | | London |   |
//! |   +--------+ |  (Spacer)  | +--------+   | 
//! |   | 14:21  | |            | | 13:21  |   |
//! |   +--------+ |            | +--------+   |
//! +--------------+------------+--------------+
//! ```
//!
//! Again, the code for that:
//!
//! ```rust
//! let mut screen = HStack::new();
//!
//! let mut berlin = VStack::new();
//! berlin.views.push(Box::new(Text::new(String::from("Berlin"), 13.0, 0)));
//! berlin.views.push(Box::new(Text::new(String::from("14:21"), 13.0, 0)));
//! berlin.padding(Edge::Left, 10);
//!
//! let mut london = VStack::new();
//! london.views.push(Box::new(Text::new(String::from("London"), 13.0, 0)));
//! london.views.push(Box::new(Text::new(String::from("13:21"), 13.0, 0)));
//! london.padding(Edge::Right, 10);
//!
//! screen.views.push(berlin);
//! screen.views.push(Box::new(Spacer::horizontal()));
//! screen.views.push(london);
//! ```

use fontdue::layout::{CoordinateSystem, Layout, TextStyle};
use fontdue;
use image::{ImageBuffer, RgbImage, ImageFormat};
use std::io::{BufReader, Read, Seek};
use std::cmp;
use std::ops::{Add, Sub};
use std::fs;
use anyhow::{Context, Result, anyhow};

/// A surface to draw on. This is really just a wrapper for [image::RgbImage],
/// which you can access using the [img] field.
pub struct Surface {
    fonts: FontCache,
    /// The underlying [image::RgbImage]. If you're implementing a [View], then
    /// you'll probably want to access this.
    pub img: RgbImage,
}

impl Surface {
    /// Create a new surface with the given dimensions.
    pub fn new(x_size: u32, y_size: u32) -> Result<Surface> {
        let roboto_data = fs::read(Surface::font_path("Roboto-Regular.ttf")?)
            .with_context(|| format!("Can't read Roboto-Regular.ttf"))?;
        let roboto = fontdue::Font::from_bytes(roboto_data, fontdue::FontSettings::default())
            .map_err(|str| anyhow!(str))?;

        let playfair_data = fs::read(Surface::font_path("PlayfairDisplay-Regular.ttf")?)
            .with_context(|| format!("Can't read PlayfairDisplay-Regular.ttf"))?;
        let playfair = fontdue::Font::from_bytes(playfair_data, fontdue::FontSettings::default())
            .map_err(|str| anyhow!(str))?;

        let mut font_cache = FontCache::new();
        font_cache.add(Font::Roboto, roboto);
        font_cache.add(Font::PlayfairDisplay, playfair);

        let mut img: RgbImage = ImageBuffer::new(x_size, y_size);
        let white = image::Rgb([255, 255, 255]);
        for y in 0..img.height() {
            for x in 0..img.width() {
                img.put_pixel(x, y, white);
            }
        }

        Ok(Surface {
            fonts: font_cache,
            img: img,
        })
    }

    pub fn bounds(&self) -> Bounds {
        Bounds::new(self.img.width(), self.img.height())
    }

    fn font_path(font: &str) -> Result<String> {
        Ok(std::env::current_dir()?
            .join("resources")
            .join(font)
            .to_str().ok_or(anyhow!("Can't convert path to string."))?
            .to_string())
    }
}

/// Internal cache for fonts. Used in [Surface]. This stores the actual fonts
/// ([fontdue::Font]) and their names ([Font]) in two separate vectors. The
/// reason for this is that the fontdue API expects a slice of [fontdue::Font]
/// and an index into that slice for selecting a font. Storing them separately
/// in here makes it easier to use with the fontdue APIs.
struct FontCache {
    fonts: Vec<fontdue::Font>,
    font_names: Vec<Font>,
}

impl FontCache {
    fn new() -> Self {
        FontCache{
            fonts: vec![],
            font_names: vec![],
        }
    }

    fn add(&mut self, name: Font, font: fontdue::Font) {
        self.fonts.push(font);
        self.font_names.push(name);
    }

    fn fonts(&self) -> &[fontdue::Font] {
        self.fonts.as_slice()
    }

    fn font(&self, name: Font) -> &fontdue::Font {
        let font_index = self.font_names.iter().position(|&n| n == name).unwrap();
        &self.fonts[font_index]
    }

    /// Construct a new [fontdue::layout::TextStyle] with the correct set of
    /// fonts and the correct font index. Used by other views, not external
    /// consumers of the module.
    fn text_style<'a>(&self, text: &'a str, size: f32, font: Font) -> TextStyle<'a> {
        let font_index = self.font_names.iter().position(|&n| n == font).unwrap(); 
        TextStyle::new(text, size, font_index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// The set of fonts that are available to use.
pub enum Font {
    Roboto,
    PlayfairDisplay,
}

/// A sizing hing for calculating the bounds of a [View]. See the remarks on
/// [View] for how to interpret this.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SizingHint {
    /// The view should size itself to its own optimal size.
    Optimal,

    /// If the view had infinite space, then how would it size itself?
    InfiniteSpace,
    
    /// If there were no space at all, how much space would the view take up?
    ZeroSpace,
}

#[derive(Clone, Copy, Debug)]
/// The bounds of a [View].
pub struct Bounds {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
    /// For bounds calculation: how should a view size itself.
    pub hint: SizingHint,
}

impl Bounds {
    /// New bounds with the given dimensions.
    ///
    /// The resulting bounds will have [SizingHint::Optimal].
    pub fn new(width: u32, height: u32) -> Self {
        Bounds { width, height, hint: SizingHint::Optimal }
    }

    /// Create a copy of the current bounds, but adjust the width.
    pub fn width_adjusted(&self, width: u32) -> Self {
        Bounds {
            width,
            height: self.height,
            hint: self.hint,
        }
    }

    /// Create a copy of the current bounds, but adjust the height.
    pub fn height_adjusted(&self, height: u32) -> Self {
        Bounds {
            width: self.width,
            height,
            hint: self.hint,
        }
    }

    /// Create a copy of the current bounds, but set the sizing hint to
    /// [SizingHint::Zero].
    pub fn zero_hinted(&self) -> Self {
        Bounds {
            width: self.width,
            height: self.height,
            hint: SizingHint::ZeroSpace,
        }
    }

    /// Create a copy of the current bounds, but set the sizing hint to
    /// [SizingHint::Optimal].
    pub fn optimally_hinted(&self) -> Self {
        Bounds {
            width: self.width,
            height: self.height,
            hint: SizingHint::Optimal,
        }
    }

    /// Create a copy of the current bounds, but set the sizing hint to
    /// [SizingHint::Infinite].
    pub fn infinitely_hinted(&self) -> Self {
        Bounds {
            width: self.width,
            height: self.height,
            hint: SizingHint::InfiniteSpace,
        }
    }

    /// Create new bounds with the current sizing hint but completely different
    /// dimensions.
    pub fn copy_hint(&self, width: u32, height: u32) -> Self {
        Bounds {
            width,
            height,
            hint: self.hint,
        }
    }
}

impl Sub for Bounds {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Bounds {
            width: self.width.checked_sub(other.width).unwrap_or(0),
            height: self.height.checked_sub(other.height).unwrap_or(0),
            hint: self.hint,
        }
    }
}

impl Add for Bounds {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Bounds {
            width: self.width + other.width,
            height: self.height + other.height,
            hint: self.hint,
        }
    }
}

impl PartialEq for Bounds {
    fn eq(&self, other: &Self) -> bool {
        self.height == other.height && self.width == other.width
    }
}

impl PartialOrd for Bounds {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.height == other.height && self.width == other.width {
            Some(std::cmp::Ordering::Equal)
        } else {
            let area_self = self.height * self.width;
            let area_other = other.height * other.width;
            if area_self > area_other {
                Some(std::cmp::Ordering::Greater)
            } else {
                Some(std::cmp::Ordering::Less)
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
/// The padding of a [View].
pub struct Padding {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

impl Padding {
    /// New padding, completely set to zero on all edges.
    fn zero() -> Padding {
        Padding {
            left: 0,
            right: 0,
            top: 0,
            bottom: 0,
        }
    }

    /// Bounds for this padding, where width equals left and right padding and
    /// height equals top and bottom padding.
    fn bounds(&self) -> Bounds {
        Bounds::new(self.left + self.right, self.top + self.bottom)
    }
}

/// Specify the edge of a view.
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

/// The basic methods that every view must implement.
pub trait View {
    /// Calculate the bounds of this view. Access to the [Surface] is provided
    /// in case the view needs to know its bounds or access other surface-level
    /// data such as fonts.
    ///
    /// The given [Bounds] are called `suggested_bounds` for a reason: a view is
    /// generally free to size itself however it wants. We do expect it to do
    /// its best to stay within the suggested bounds, of course. But if an image
    /// is 100 pixels width and 100 pixels high, then we don't expect the view
    /// to crop or resize the image if the suggested bounds are 50x50. It's the
    /// callers to job to verify that everything fits within the given
    /// dimensions.
    ///
    /// In addition to a suggestion for the width and height, the suggested
    /// bounds also contain a sizing hint. The sizing hint can be zero, optimal,
    /// or infinite (see [SizingHint]). The view should use this sizing hint to
    /// decide how much space to take up, if it's flexible.
    ///
    /// An image, for example, is not usually flexible. A [Spacer], however,
    /// might be very flexible: for zero-hinted suggested bounds, it may decide
    /// to collapse all the way down to 0x0 pixels. In an optimally- or
    /// infinitely-hinted scenario, it will take up all the suggested space.
    ///
    /// The difference between zero-hinted and optimally-/infinitely-hinted is
    /// pretty clear: in a zero-hinted scenario, the view should try to take up
    /// as little space as possible. The optimally-hinted vs. infinitely-hinted
    /// cases are not as intuitive: in the optimally-hinted case, the view
    /// should take up exactly as much space as it requires to display its
    /// content as intended. In the infinitely-hinted scenario, it's free to
    /// take up even more space, if it makes sense for that particular view.
    ///
    /// Views should expect `bounds` to be called multiple times before a call
    /// to `draw`. Usually these calls will have different sizing hints. This is
    /// because [VStack] and [HStack], for example, will try to determine which
    /// of its child views are the most and least flexible to partition their
    /// available space optimally.
    fn bounds(&self, surface: &Surface, suggested_bounds: Bounds) -> Bounds;

    /// Draw the contents of this view to the given [Surface] at the given
    /// coordinates. Try to stick to the suggested bounds, although those are
    /// really just a suggestion, see [bounds] for more information.
    fn draw(&self, surface: &mut Surface, x: u32, y: u32, suggested_bounds: Bounds);

    /// Read the padding data for this view. This is usually really just a
    /// getter.
    fn padding_data(&self) -> Padding;

    /// Set the padding data for this view. This is usually really just a
    /// setter.
    fn set_padding_data(&mut self, padding: Padding);

    /// Modify the current padding by setting the padding for `edge` to `size`
    /// pixels. This will replace the padding for the given [Edge], but not
    /// modify the padding for all other edges.
    fn padding(&mut self, edge: Edge, size: u32) {
        let mut new_padding = self.padding_data();
        match edge {
            Edge::Left => new_padding.left = size,
            Edge::Right => new_padding.right = size,
            Edge::Top => new_padding.top = size,
            Edge::Bottom => new_padding.bottom = size,
        };

        self.set_padding_data(new_padding);
    }
}

/// Horizontal alignment.
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// A vertical stack of views.
///
/// See the module-level documentation for usage examples.
///
/// The VStack will distribute space to all its child views as fairly as it can:
/// it tries to divide the available space so the distribution matches the child
/// views' willingness to "flex", i.e., how much they can size up/down. For
/// example: an image or text is not very flexible -- it takes as much space as
/// it takes to render those -- but a [Spacer] is maximally flexible: it can
/// scale down to zero and up to an arbitrary size.
///
/// You can use [align] to set the horizontal alignment of child views (see
/// [HAlign]). [spacing] determines how much space to leave between the elements
/// of a VStack.
///
/// Access the `views` field directly to manage the child views.
pub struct VStack {
    pub views: Vec<Box<dyn View>>,
    pub spacing: u32,
    pub align: HAlign,
    padding: Padding,
}

impl VStack {
    /// Create a new vertical stack. This sets the alignment to [HAlign::Left].
    pub fn new() -> VStack {
        VStack {
            views: Vec::new(),
            spacing: 0,
            align: HAlign::Left,
            padding: Padding::zero(),
        }
    }

    fn placements_and_heights(&self, surface: &Surface, suggested_bounds: Bounds) -> Vec<(usize, u32, u32)> {
        // The idea of a stack is that views have varying levels of flexibility
        // when it comes to their height. Regular text views have pretty much
        // no flexibility: they need enough space to fit the text, but not more.
        // Spacers, on the other hand, can collapse to zero height or expand to
        // fill all available space.
        //
        // First, we test our child views for their flexibility, ranking them by
        // the amount that they're willing to flex. We then start assigning
        // height, beginning with the least flexible of our views. Our initial
        // suggestion is the total space we have available divided by the
        // number of views — we distribute the space equally.
        //
        // Our child view may opt to use that exact amount of space, or it may
        // take less or require more. Whatever the case may be: we subtract the
        // height it has chosen for itself from the available height and
        // continue the process with the next least flexible view.
        
        let mut result = Vec::<(usize, u32, u32)>::new();

        let mut flexibility: Vec<(u32, usize)> = self.views
            .iter()
            .map(|v| {
                let mut score = 0;
                if v.bounds(surface, Bounds::new(999, 999).infinitely_hinted()).height == 999 {
                    score += 3;
                }
                let zero_height = v.bounds(surface, Bounds::new(0, 0).zero_hinted()).height;
                if zero_height == 0 {
                    score += 3;
                }
                let optimal_height = v.bounds(surface, Bounds::new(999, 999).optimally_hinted()).height;
                if zero_height > 0 && zero_height < optimal_height {
                    // Some willingness to flex down.
                    score += 2;
                }

                score
            })
            .zip(0..self.views.len())
            .collect();
        flexibility.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // To determine our initial height, ask all child views for how much
        // space they want giving the sizing hint in `suggested_bounds`. Then
        // clamp that value to a maximum of the suggested height. After that, we
        // still have to account for the height required by inter-element
        // spacing.
        let initial_height_from_views = match suggested_bounds.hint {
            SizingHint::Optimal => {
                // If we want to optimally size, then ask all of our child views
                // to size themselves optimally using the bounds that we got
                // initially. Sum those values, then clamp that sum to the
                // height of the suggested bounds. Divvying up our own height at
                // this point would potentially lead to subviews under-reporting
                // their optimal height. We'll figure out the real values later.
                self.views.iter().map(|v| v.bounds(surface, suggested_bounds).height).sum()
            },
            SizingHint::ZeroSpace => {
                // To zero-size, ask all of our children to zero-size and then
                // add that up.
                self.views.iter().map(|v| v.bounds(surface, suggested_bounds).height).sum()
            },
            SizingHint::InfiniteSpace => {
                // Always use up the maximum height if we have infinite space.
                suggested_bounds.height
            },
        };
        let mut initial_height = cmp::min(suggested_bounds.height, initial_height_from_views);
        let spacing_height = if self.views.len() > 0 {
            (self.views.len() as u32 - 1) * self.spacing
        } else {
            0
        };
        if initial_height > suggested_bounds.height - spacing_height {
            initial_height = suggested_bounds.height - spacing_height;
        }

        let mut leftover_height = initial_height;
        let mut temp_heights = Vec::<(usize, u32)>::new();
        for i in 0..flexibility.len() {
            let view_index = flexibility.get(i).unwrap().1;
            let view = self.views.get(view_index).unwrap();
            let views_left = (flexibility.len() - i) as u32;
            let suggestion = leftover_height / views_left;
            let actual_height = view
                .bounds(surface, suggested_bounds.height_adjusted(suggestion))
                .height;

            temp_heights.push((view_index, actual_height));

            if (leftover_height as i32) - (actual_height as i32) < 0 {
                leftover_height = 0;
            } else {
                leftover_height -= actual_height;
            }
        }

        let mut y_off = 0;
        for i in 0..self.views.len() {
            let view_index = i;
            let height = temp_heights.iter().find(|h| h.0 == view_index).unwrap().1;
            result.push((view_index, y_off, height));
            y_off += height + self.spacing;
        }

        result
    }
}

impl View for VStack {
    fn bounds(&self, surface: &Surface, suggested_bounds: Bounds) -> Bounds {
        // Maximum width of all child views should suffice.
        let unpadded_width = self.views
            .iter()
            .map(|v| v.bounds(surface, suggested_bounds).width)
            .max()
            .unwrap_or(0);
        let width = unpadded_width + self.padding_data().left + self.padding_data().right;

        let placement_bounds = suggested_bounds - self.padding_data().bounds();

        let placements = self.placements_and_heights(surface, placement_bounds);
        let last_view = placements.last().unwrap_or(&(0, 0, 0));
        let total_height = last_view.1 + last_view.2 + self.padding_data().top + self.padding_data().bottom;

        Bounds::new(width, total_height)
    }

    fn draw(&self, surface: &mut Surface, x: u32, y: u32, suggested_bounds: Bounds) {
        let max_x = x + suggested_bounds.width - self.padding_data().left - self.padding_data().right;

        let placement_bounds = suggested_bounds - self.padding_data().bounds();
        let placements = self.placements_and_heights(surface, placement_bounds);

        for i in 0..self.views.len() {
            let view = self.views.get(i).unwrap();
            let placement = placements.get(i).unwrap();

            let placed_bounds = suggested_bounds.copy_hint(
                placement_bounds.width,
                placement.2,
            );

            // Ask the child view for its bound to figure out the horizontal
            // aligment.
            let child_bounds = view.bounds(surface, placed_bounds);

            let view_x = match self.align {
                HAlign::Left => x + self.padding_data().left,
                HAlign::Right => max_x - child_bounds.width,
                HAlign::Center => x + self.padding_data().left + (suggested_bounds.width - child_bounds.width) / 2,
            };

            view.draw(surface, view_x, y + self.padding_data().top + placement.1, child_bounds);
        }
    }

    fn padding_data(&self) -> Padding {
        self.padding
    }

    fn set_padding_data(&mut self, new_padding: Padding) {
        self.padding = new_padding;
    }
}

/// Vertical alignment.
pub enum VAlign {
    Top,
    Center,
    Bottom,
}

/// A horizontal stack of views.
///
/// See the module-level documentation for usage examples.
///
/// The HStack will distribute space to all its child views as fairly as it can:
/// it tries to divide the available space so the distribution matches the child
/// views' willingness to "flex", i.e., how much they can size up/down. For
/// example: an image or text is not very flexible -- it takes as much space as
/// it takes to render those -- but a [Spacer] is maximally flexible: it can
/// scale down to zero and up to an arbitrary size.
///
/// You can use [align] to set the vertical alignment of child views (see
/// [VAlign]). [spacing] determines how much space to leave between the elements
/// of a VStack.
///
/// Access the `views` field directly to manage the child views.
pub struct HStack {
    pub views: Vec<Box<dyn View>>,
    pub spacing: u32,
    pub align: VAlign,
    padding: Padding,
}

impl HStack {
    pub fn new() -> Self {
        HStack {
            views: Vec::new(),
            spacing: 0,
            align: VAlign::Top,
            padding: Padding::zero(),
        }
    }

    fn placements_and_widths(&self, surface: &Surface, suggested_bounds: Bounds) -> Vec<(usize, u32, u32)> {
        // See the `placements_and_heights` in `VStack` for an explanation. This
        // does the same thing, but with width instead of height.

        let mut result = Vec::<(usize, u32, u32)>::new();

        let mut flexibility: Vec<(u32, usize)> = self.views
            .iter()
            .map(|v| {
                let mut score = 0;
                if v.bounds(surface, Bounds::new(999, 999).infinitely_hinted()).width == 999 {
                    score += 3;
                }
                let zero_width = v.bounds(surface, Bounds::new(0, 0).zero_hinted()).width;
                if zero_width == 0 {
                    score += 3;
                }
                let optimal_width = v.bounds(surface, Bounds::new(999, 999).optimally_hinted()).width;
                if zero_width > 0 && zero_width < optimal_width {
                    // Some willingness to flex down.
                    score += 2;
                }

                score
            })
            .zip(0..self.views.len())
            .collect();
        flexibility.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // To determine our initial width, ask all child views for how much
        // space they want giving the sizing hint in `suggested_bounds`. Then
        // clamp that value to a maximum of the suggested width. After that, we
        // still have to account for the width required by inter-element
        // spacing.
        let initial_width_from_views = match suggested_bounds.hint {
            SizingHint::Optimal => {
                // If we want to optimally size, then ask all of our child views
                // to size themselves optimally using the bounds that we got
                // initially. Sum those values, then clamp that sum to the
                // width of the suggested bounds. Divvying up our own width at
                // this point would potentially lead to subviews under-reporting
                // their optimal width. We'll figure out the real values later.
                self.views.iter().map(|v| v.bounds(surface, suggested_bounds).width).sum()
            },
            SizingHint::ZeroSpace => {
                // To zero-size, ask all of our children to zero-size and then
                // add that up.
                self.views.iter().map(|v| v.bounds(surface, suggested_bounds).width).sum()
            },
            SizingHint::InfiniteSpace => {
                // Always use up the maximum height if we have infinite space.
                suggested_bounds.width
            },
        };
        let mut initial_width = cmp::min(suggested_bounds.width, initial_width_from_views);
        let spacing_width = if self.views.len() > 0 {
            (self.views.len() as u32 - 1) * self.spacing
        } else {
            0
        };
        if initial_width > suggested_bounds.width - spacing_width {
            initial_width = suggested_bounds.width - spacing_width;
        }

        let mut leftover_width = initial_width;
        let mut temp_widths = Vec::<(usize, u32)>::new();
        for i in 0..flexibility.len() {
            let view_index = flexibility.get(i).unwrap().1;
            let view = self.views.get(view_index).unwrap();
            let views_left = (flexibility.len() - i) as u32;
            let suggestion = leftover_width / views_left;
            let actual_width = view
                .bounds(surface, suggested_bounds.width_adjusted(suggestion))
                .width;

            temp_widths.push((view_index, actual_width));

            if (leftover_width as i32) - (actual_width as i32) < 0 {
                leftover_width = 0;
            } else {
                leftover_width -= actual_width;
            }
        }

        let mut x_off = 0;
        for i in 0..self.views.len() {
            let view_index = i;
            let width = temp_widths.iter().find(|h| h.0 == view_index).unwrap().1;
            result.push((view_index, x_off, width));
            x_off += width + self.spacing;
        }

        result
    }
}

impl View for HStack {
    fn bounds(&self, surface: &Surface, suggested_bounds: Bounds) -> Bounds {
        // Maximum height of all child views should suffice.
        let unpadded_height = self.views
            .iter()
            .map(|v| v.bounds(surface, suggested_bounds).height)
            .max()
            .unwrap_or(0);
        let height = unpadded_height + self.padding_data().top + self.padding_data().bottom;

        let placement_bounds = suggested_bounds - self.padding_data().bounds();

        let placements = self.placements_and_widths(surface, placement_bounds);
        let last_view = placements.last().unwrap_or(&(0, 0, 0));
        let total_width = last_view.1 + last_view.2 + self.padding_data().left + self.padding_data().right;

        Bounds::new(total_width, height)
    }

    fn draw(&self, surface: &mut Surface, x: u32, y: u32, suggested_bounds: Bounds) {
        let max_y = y + suggested_bounds.height - self.padding_data().top - self.padding_data().bottom;

        let placement_bounds = suggested_bounds - self.padding_data().bounds();
        let placements = self.placements_and_widths(surface, placement_bounds);

        for i in 0..self.views.len() {
            let view = self.views.get(i).unwrap();
            let placement = placements.get(i).unwrap();

            let placed_bounds = suggested_bounds.copy_hint(
                placement.2,
                placement_bounds.height,
            );

            // Ask the child view for its bound to figure out the horizontal
            // aligment.
            let child_bounds = view.bounds(surface, placed_bounds);

            let view_y = match self.align {
                VAlign::Top => y + self.padding_data().top,
                VAlign::Bottom => max_y - child_bounds.height,
                VAlign::Center => y + self.padding_data().top + (suggested_bounds.height - child_bounds.height) / 2,
            };

            view.draw(surface, x + self.padding_data().left + placement.1, view_y, child_bounds);
        }
    }

    fn padding_data(&self) -> Padding {
        self.padding
    }

    fn set_padding_data(&mut self, new_padding: Padding) {
        self.padding = new_padding;
    }
}

enum Direction {
    Horizontal,
    Vertical,
}

/// A spacer takes up all the available space while being maximally flexible: it
/// will never cut into space that other views require, but will fill all truly
/// available space.
///
/// Each spacer is flexible in one direction -- horizontally or vertically --
/// and takes up zero space in the other.
/// 
/// It's usually used in conjunction with layout views such as [VStack] and
/// [HStack].
pub struct Spacer {
    direction: Direction,
}

impl Spacer {
    /// Create a new horizontal spacer.
    pub fn horizontal() -> Self {
        Spacer { direction: Direction::Horizontal }
    }

    /// Create a new vertical spacer.
    pub fn vertical() -> Self {
        Spacer { direction: Direction::Vertical }
    }
}

impl View for Spacer {
    fn bounds(&self, _surface: &Surface, suggested_bounds: Bounds) -> Bounds {
        // A spacer takes up all the available space. That means that we just
        // completely use up the available bounds, taking sizing hints into
        // account.

        if suggested_bounds.hint == SizingHint::ZeroSpace {
            Bounds::new(0, 0)
        } else {
            match self.direction {
                Direction::Vertical => suggested_bounds.width_adjusted(0),
                Direction::Horizontal => suggested_bounds.height_adjusted(0),
            }
        }
    }

    fn draw(&self, _surface: &mut Surface, _x: u32, _y: u32, _suggested_bounds: Bounds) {
    }

    fn padding_data(&self) -> Padding {
        Padding::zero()
    }

    fn set_padding_data(&mut self, _: Padding) {
    }
}

/// Renders text.
///
/// [Text] currently supports arbitrary font sizes and font wrapping. The choice
/// of fonts is limited, see [Font]. 
pub struct Text {
    /// The text to render.
    pub text: String,

    /// The font size to use.
    pub size: f32,
    
    /// The font to use.
    pub font: Font,

    /// Wrap text to best fit the width of the suggested bounds. This wraps on
    /// word boundaries. To really fit the suggested bounds, you'll probably
    /// want to reduce font sizes until [bounds] returns something that you're
    /// happy with.
    pub wrap_text: bool,

    padding: Padding,
}

impl Text {
    pub fn new(text: String, size: f32, font: Font) -> Text {
        Text {
            text,
            size,
            font,
            padding: Padding::zero(),
            wrap_text: false,
        }
    }

    fn set_up_wrapping(&self, layout: &mut Layout, suggested_bounds: Bounds) {
        let mut settings = layout.settings().clone();
        settings.max_width = Some((suggested_bounds - self.padding_data().bounds()).width as f32);
        layout.reset(&settings);
    }
}

impl View for Text {
    fn bounds(&self, surface: &Surface, suggested_bounds: Bounds) -> Bounds {
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        if self.wrap_text {
            self.set_up_wrapping(&mut layout, suggested_bounds);
        }
        layout.append(
            surface.fonts.fonts(),
            &surface.fonts.text_style(self.text.as_str(), self.size, self.font),
        );

        // Find the extent on the X and Y axes.
        let glyphs = layout.glyphs();
        let mut max_x: u32 = 0;
        let mut max_y: u32 = 0;
        for glyph in glyphs {
            let right_edge = glyph.x as u32 + glyph.width as u32;
            let bottom_edge = glyph.y as u32 + glyph.height as u32;
            if right_edge > max_x {
                max_x = right_edge;
            }
            if bottom_edge > max_y {
                max_y = bottom_edge;
            }
        }

        // Now add padding.
        max_x += self.padding_data().left + self.padding_data().right;
        max_y += self.padding_data().top + self.padding_data().bottom;

        Bounds::new(max_x, max_y)
    }

    fn draw(&self, surface: &mut Surface, origin_x: u32, origin_y: u32, suggested_bounds: Bounds) {
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        if self.wrap_text {
            self.set_up_wrapping(&mut layout, suggested_bounds);
        }
        layout.append(
            surface.fonts.fonts(),
            &surface.fonts.text_style(self.text.as_str(), self.size, self.font),
        );
        let glyphs = layout.glyphs();

        let pad_origin_x = origin_x + self.padding_data().left;
        let pad_origin_y = origin_y + self.padding_data().top;

        let font_data = surface.fonts.font(self.font);

        for glyph in glyphs {
            let (metrics, bitmap) =
                font_data.rasterize_indexed(glyph.key.glyph_index, glyph.key.px);

            for y in 0..metrics.height {
                for x in 0..metrics.width {
                    let opacity = bitmap[y * metrics.width + x];
                    let pixel_x: u32 = (glyph.x as u32 + x as u32 + pad_origin_x).try_into().unwrap();
                    let pixel_y: u32 = (glyph.y as u32 + y as u32 + pad_origin_y).try_into().unwrap();
                    if opacity > 30 {
                        surface.img.put_pixel(
                            pixel_x,
                            pixel_y,
                            image::Rgb([/*255 - opacity*/ 0, 0, 0]),
                        );
                    }
                }
            }
        }
    }

    fn padding_data(&self) -> Padding {
        self.padding
    }

    fn set_padding_data(&mut self, new_padding: Padding) {
        self.padding = new_padding;
    }
}

/// Renders an image.
pub struct Image {
    image_data: RgbImage,
    padding: Padding,
}

impl Image {
    /// Create a new image from the given reader ([std::io::Read]). The image
    /// has to be a PNG.
    pub fn from_data<R: Read + Seek>(data: R) -> Result<Image> {
        let img = image::ImageReader::with_format(
            BufReader::new(data),
            ImageFormat::Png
        )
        .decode()
        .with_context(|| "Error decoding image.")?;

        Ok(Image {
            image_data: img.into(),
            padding: Padding::zero(),
        })
    }
}

impl View for Image {
    fn bounds(&self, _surface: &Surface, _suggested_bounds: Bounds) -> Bounds {
        // Images need the space they need. No more, no less.
        Bounds::new(self.image_data.width(), self.image_data.height()) +
            self.padding_data().bounds() 
    }

    fn draw(&self, surface: &mut Surface, x: u32, y: u32, _suggested_bounds: Bounds) {
        let pad_origin_x = x + self.padding_data().left;
        let pad_origin_y = y + self.padding_data().left;

        for img_y in 0..self.image_data.height() {
            for img_x in 0..self.image_data.width() {
                surface.img.put_pixel(
                    pad_origin_x + img_x,
                    pad_origin_y + img_y,
                    *self.image_data.get_pixel(img_x, img_y)
                );
            }
        }
    }

    fn padding_data(&self) -> Padding {
        self.padding
    }

    fn set_padding_data(&mut self, new_padding: Padding) {
        self.padding = new_padding;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use std::cell::RefCell;

    struct DrawingRegister {
        // id, x, y
        drawings: RefCell<Vec<(u32, u32, u32, Bounds)>>,
    }

    impl DrawingRegister {
        pub fn new() -> DrawingRegister {
            DrawingRegister {
                drawings: RefCell::new(Vec::new()),
            }
        }

        pub fn register(&self, view_id: u32, x: u32, y: u32, suggested_bounds: Bounds) {
            self.drawings.borrow_mut().push((view_id, x, y, suggested_bounds));
        }
        
        pub fn was_drawn_at(&self, view_id: u32, x: u32, y: u32) -> bool {
            for d in self.drawings.borrow().iter() {
                if d.0 == view_id && d.1 == x && d.2 == y {
                    return true;
                }
            }

            false
        }

        pub fn was_drawn_with_bounds(&self, view_id: u32, bounds: Bounds) -> bool {
            for d in self.drawings.borrow().iter() {
                if d.0 == view_id && d.3.width == bounds.width && d.3.height == bounds.height {
                    return true;
                }
            }

            false
        }
    }

    struct MonitorWrapper {
        pub id: u32,
        pub drawing_register: Rc<DrawingRegister>,
        pub child: Box<dyn View>,
    }

    impl MonitorWrapper {
        pub fn new(id: u32, drawing_register: Rc<DrawingRegister>, child: Box<dyn View>) -> Self {
            MonitorWrapper { id, drawing_register, child }
        }
    }

    impl View for MonitorWrapper {
        fn bounds(&self, surface: &Surface, suggested_bounds: Bounds) -> Bounds {
            self.child.bounds(surface, suggested_bounds)
        }

        fn draw(&self, surface: &mut Surface, x: u32, y: u32, suggested_bounds: Bounds) {
            self.drawing_register.as_ref().register(self.id, x, y, suggested_bounds);
            self.child.draw(surface, x, y, suggested_bounds);
        }

        fn padding_data(&self) -> Padding {
            Padding::zero()
        }

        fn set_padding_data(&mut self, _: Padding) {
        }
    }

    struct TestView {
        pub id: u32,
        pub width: u32,
        pub height: u32,
        pub drawing_register: Option<Rc<DrawingRegister>>,
        pub padding: Padding,
    }

    impl TestView {
        pub fn new(width: u32, height: u32) -> TestView {
            TestView {
                id: 0,
                width,
                height,
                drawing_register: None,
                padding: Padding::zero(),
            }
        }

        pub fn monitored(id: u32, register: Rc<DrawingRegister>, width: u32, height: u32) -> TestView {
            TestView {
                id,
                width,
                height,
                drawing_register: Some(register),
                padding: Padding::zero(),
            }
        }
    }

    impl View for TestView {
        fn bounds(&self, surface: &Surface, suggested_bounds: Bounds) -> Bounds {
            Bounds::new(self.width, self.height)
        }

        fn draw(&self, surface: &mut Surface, x: u32, y: u32, suggested_bounds: Bounds) {
            if self.drawing_register.is_none() {
                return;
            }

            self.drawing_register.as_ref().unwrap().register(self.id, x, y, suggested_bounds);
        }

        fn padding_data(&self) -> Padding {
            self.padding
        }

        fn set_padding_data(&mut self, new_padding: Padding) {
            self.padding = new_padding;
        }
    }

    #[test]
    fn test_empty_vstack_has_zero_width() {
        let surface = Surface::new(300, 300).unwrap();
        let vstack = VStack::new();
        assert_eq!(0, vstack.bounds(&surface, surface.bounds()).width);
    }

    #[test]
    fn test_vstack_has_maximum_width_of_all_children() {
        let surface = Surface::new(300, 300).unwrap();
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::new(50, 10)));
        vstack.views.push(Box::new(TestView::new(150, 10)));
        vstack.views.push(Box::new(TestView::new(100, 10)));
        assert_eq!(150, vstack.bounds(&surface, surface.bounds()).width);
    }

    #[test]
    fn test_empty_vstack_has_zero_height() {
        let surface = Surface::new(300, 300).unwrap();
        let mut vstack = VStack::new();
        assert_eq!(0, vstack.bounds(&surface, surface.bounds()).height);
    }

    #[test]
    fn test_vstack_has_total_height_of_all_children() {
        let surface = Surface::new(300, 300).unwrap();
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::new(50, 50)));
        vstack.views.push(Box::new(TestView::new(50, 100)));
        vstack.views.push(Box::new(TestView::new(50, 10)));
        assert_eq!(160, vstack.bounds(&surface, surface.bounds()).height);
    }

    #[test]
    fn test_vstack_height_includes_spacing() {
        let surface = Surface::new(300, 300).unwrap();
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::new(50, 50)));
        vstack.views.push(Box::new(TestView::new(50, 100)));
        vstack.views.push(Box::new(TestView::new(50, 10)));
        vstack.spacing = 5;
        assert_eq!(170, vstack.bounds(&surface, surface.bounds()).height);
    }
    
    #[test]
    fn test_vstack_draws_left_aligned_elements_at_original_x() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 50, 100)));
        let bounds = surface.bounds();
        vstack.draw(&mut surface, 100, 100, bounds);
        assert!(register.was_drawn_at(1, 100, 100));
        assert!(register.was_drawn_at(2, 100, 150));
    }

    #[test]
    fn test_vstack_draws_right_aligned_elements() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 75, 100)));
        vstack.align = HAlign::Right;
        let bounds = surface.bounds();
        vstack.draw(&mut surface, 100, 100, bounds - Bounds::new(100, 100));
        assert!(register.was_drawn_at(1, 450, 100));
        assert!(register.was_drawn_at(2, 425, 150));
    }

    #[test]
    fn test_vstack_draws_center_aligned_elements() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 75, 100)));
        vstack.align = HAlign::Center;
        let bounds = surface.bounds();
        vstack.draw(&mut surface, 100, 100, bounds - Bounds::new(100, 100));
        assert!(register.was_drawn_at(1, 275, 100));
        assert!(register.was_drawn_at(2, 262, 150));
    }

    #[test]
    fn test_vstack_leaves_spacing_between_elements() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 75, 100)));
        vstack.spacing = 10;
        let bounds = surface.bounds();
        vstack.draw(&mut surface, 100, 100, bounds - Bounds::new(100, 100));
        assert!(register.was_drawn_at(1, 100, 100));
        assert!(register.was_drawn_at(2, 100, 160));
    }

    #[test]
    // TODO: Find a better name for this.
    fn test_vstack_spacer() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 75, 100)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        assert_eq!(150, vstack.bounds(&surface, bounds.zero_hinted()).height);
        assert_eq!(bounds.height, vstack.bounds(&surface, bounds.optimally_hinted()).height);
        assert_eq!(bounds.height, vstack.bounds(&surface, bounds.infinitely_hinted()).height);
    }

    #[test]
    fn test_vstack_layouts_zero_views() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 0)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 75, 100)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        assert_eq!(100, vstack.bounds(&surface, bounds.zero_hinted()).height);
        assert_eq!(bounds.height, vstack.bounds(&surface, bounds.optimally_hinted()).height);
        assert_eq!(bounds.height, vstack.bounds(&surface, bounds.infinitely_hinted()).height);
    }

    #[test]
    fn test_vstack_layouts_views_that_are_too_big() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 100)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 50, 100)));
        let bounds = Bounds::new(50, 50);
        assert_eq!(200, vstack.bounds(&surface, bounds.zero_hinted()).height);
        assert_eq!(200, vstack.bounds(&surface, bounds.optimally_hinted()).height);
        assert_eq!(200, vstack.bounds(&surface, bounds.infinitely_hinted()).height);
    }

    #[test]
    fn test_vstack_layouts_multiple_spacers_zero_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 100)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 50, 75)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(3, register.clone(), 50, 50)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Zero-hinted, so both spacers should just collapse.
        assert_eq!(225, vstack.bounds(&surface, bounds.zero_hinted()).height);
        vstack.draw(&mut surface, 0, 0, bounds.zero_hinted());
        assert!(register.was_drawn_at(1, 0, 0));
        assert!(register.was_drawn_at(2, 0, 100));
        assert!(register.was_drawn_at(3, 0, 175));
    }

    #[test]
    fn test_vstack_layouts_multiple_spacers_optimally_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 100)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 50, 75)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(3, register.clone(), 50, 50)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Optimally-hinted, so let spacers expand up to max.
        assert_eq!(bounds.height, vstack.bounds(&surface, bounds.optimally_hinted()).height);
        vstack.draw(&mut surface, 0, 0, bounds.optimally_hinted());
        // The first one is just at the top. After that, we'd expect the first
        // and second spacers to take up equal height, so 87 and 88 pixels.
        // (400 - (100 + 75 + 50)) / 2 = (400 - 225) / 2 = 175 / 2 = 87.5
        assert!(register.was_drawn_at(1, 0, 0));
        assert!(register.was_drawn_at(2, 0, 187));
        assert!(register.was_drawn_at(3, 0, 350));
    }

    #[test]
    fn test_vstack_layouts_multiple_spacers_infinitely_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 100)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 50, 75)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(3, register.clone(), 50, 50)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Infinitely-hinted, so let spacers expand up to max.
        assert_eq!(bounds.height, vstack.bounds(&surface, bounds.infinitely_hinted()).height);
        vstack.draw(&mut surface, 0, 0, bounds.infinitely_hinted());
        // The first one is just at the top. After that, we'd expect the first
        // and second spacers to take up equal height, so 87 and 88 pixels.
        // (400 - (100 + 75 + 50)) / 2 = (400 - 225) / 2 = 175 / 2 = 87.5
        assert!(register.was_drawn_at(1, 0, 0));
        assert!(register.was_drawn_at(2, 0, 187));
        assert!(register.was_drawn_at(3, 0, 350));
    }

    #[test]
    fn test_vstack_layouts_purely_spacers_zero_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(Spacer::vertical()));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Zero-hinted, so the spacers should collapse to zero, which means the
        // entire VStack collapses to zero.
        assert_eq!(0, vstack.bounds(&surface, bounds.zero_hinted()).height);
    }

    #[test]
    fn test_vstack_layouts_purely_spacers_optimally_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(Spacer::vertical()));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Optimally-hinted, so spacers expand to the maximum possible.
        assert_eq!(bounds.height, vstack.bounds(&surface, bounds.optimally_hinted()).height);
    }

    #[test]
    fn test_vstack_layouts_purely_spacers_infinitely_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(Spacer::vertical()));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Infinitely-hinted, so spacers expand to the maximum possible.
        assert_eq!(bounds.height, vstack.bounds(&surface, bounds.infinitely_hinted()).height);
    }

    #[test]
    fn test_vstack_layouts_nested_vstack() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut inner_vstack = VStack::new();
        inner_vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        inner_vstack.views.push(Box::new(Spacer::vertical()));
        inner_vstack.views.push(Box::new(TestView::monitored(2, register.clone(), 80, 80)));
        let mut outer_vstack = VStack::new();
        outer_vstack.views.push(Box::new(inner_vstack));
        outer_vstack.views.push(Box::new(TestView::monitored(3, register.clone(), 100, 100)));
        outer_vstack.views.push(Box::new(MonitorWrapper::new(4, register.clone(), Box::new(Spacer::vertical()))));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        outer_vstack.draw(&mut surface, 0, 0, bounds.infinitely_hinted());
        // First, the least-flexible view — the test view 3 of height 100 — will
        // be offered 133 pixels, but it'll only need 100 pixels. Then, 150 of
        // the remaining 300 pixels will be offered to the inner vstack, since
        // that's _slightly_ less flexible than the bottom spacer. It'll gladly
        // take the entire 150 pixels, expanding its spacer to 20 pixels. After
        // that, the bottom spacer will get the remaining 150 pixels.
        assert!(register.was_drawn_at(1, 0, 0));
        assert!(register.was_drawn_at(2, 0, 70));
        assert!(register.was_drawn_at(3, 0, 150));
        assert!(register.was_drawn_at(4, 0, 250));
    }

    #[test]
    fn test_empty_hstack_has_zero_height() {
        let surface = Surface::new(300, 300).unwrap();
        let hstack = HStack::new();
        assert_eq!(0, hstack.bounds(&surface, surface.bounds()).height);
    }

    #[test]
    fn test_empty_hstack_has_maximum_height_of_all_children() {
        let surface = Surface::new(300, 300).unwrap();
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::new(10, 50)));
        hstack.views.push(Box::new(TestView::new(10, 150)));
        hstack.views.push(Box::new(TestView::new(10, 100)));
        assert_eq!(150, hstack.bounds(&surface, surface.bounds()).height);
    }

    #[test]
    fn test_empty_hstack_has_zero_width() {
        let surface = Surface::new(300, 300).unwrap();
        let mut hstack = HStack::new();
        assert_eq!(0, hstack.bounds(&surface, surface.bounds()).width);
    }

    #[test]
    fn test_hstack_with_inflexible_children_expands_to_their_total_width() {
        let surface = Surface::new(300, 300).unwrap();
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::new(50, 50)));
        hstack.views.push(Box::new(TestView::new(100, 50)));
        hstack.views.push(Box::new(TestView::new(10, 50)));
        assert_eq!(160, hstack.bounds(&surface, surface.bounds()).width);
    }

    #[test]
    fn test_hstack_width_includes_spacing() {
        let surface = Surface::new(300, 300).unwrap();
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::new(50, 50)));
        hstack.views.push(Box::new(TestView::new(100, 50)));
        hstack.views.push(Box::new(TestView::new(10, 50)));
        hstack.spacing = 5;
        assert_eq!(170, hstack.bounds(&surface, surface.bounds()).width);
    }

    #[test]
    fn test_hstack_draws_top_aligned_elements_at_original_y() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 100, 50)));
        let bounds = surface.bounds();
        hstack.draw(&mut surface, 100, 100, bounds);
        assert!(register.was_drawn_at(1, 100, 100));
        assert!(register.was_drawn_at(2, 150, 100));
    }

    #[test]
    fn test_hstack_draws_bottom_aligned_elements() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 100, 75)));
        hstack.align = VAlign::Bottom;
        let bounds = surface.bounds() - Bounds::new(100, 100);
        hstack.draw(&mut surface, 100, 100, bounds);
        assert!(register.was_drawn_at(1, 100, 450));
        assert!(register.was_drawn_at(2, 150, 425));
    }

    #[test]
    fn test_hstack_draws_center_aligned_elements() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 100, 75)));
        hstack.align = VAlign::Center;
        let bounds = surface.bounds() - Bounds::new(100, 100);
        hstack.draw(&mut surface, 100, 100, bounds);
        assert!(register.was_drawn_at(1, 100, 275));
        assert!(register.was_drawn_at(1, 150, 262));
    }

    #[test]
    fn test_hstack_leaves_spacing_between_elements() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 100, 75)));
        hstack.spacing = 10;
        let bounds = surface.bounds() - Bounds::new(100, 100);
        hstack.draw(&mut surface, 100, 100, bounds);
        assert!(register.was_drawn_at(1, 100, 100));
        assert!(register.was_drawn_at(2, 160, 100));
    }

    #[test]
    fn test_hstack_expands_spacers() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 100, 75)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        assert_eq!(150, hstack.bounds(&surface, bounds.zero_hinted()).width);
        assert_eq!(bounds.width, hstack.bounds(&surface, bounds.optimally_hinted()).width);
        assert_eq!(bounds.width, hstack.bounds(&surface, bounds.infinitely_hinted()).width);
    }

    #[test]
    fn test_hstack_layouts_zero_views() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 0, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 100, 75)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        assert_eq!(100, hstack.bounds(&surface, bounds.zero_hinted()).width);
        assert_eq!(bounds.width, hstack.bounds(&surface, bounds.optimally_hinted()).width);
        assert_eq!(bounds.width, hstack.bounds(&surface, bounds.infinitely_hinted()).width);
    }

    #[test]
    fn test_hstack_layouts_views_that_are_too_big() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 100, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 100, 50)));
        let bounds = Bounds::new(50, 50);
        assert_eq!(200, hstack.bounds(&surface, bounds.zero_hinted()).width);
        assert_eq!(200, hstack.bounds(&surface, bounds.optimally_hinted()).width);
        assert_eq!(200, hstack.bounds(&surface, bounds.infinitely_hinted()).width);
    }

    #[test]
    fn test_hstack_layouts_multiple_spacers_zero_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 100, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 75, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(3, register.clone(), 50, 50)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Zero-hinted, so both spacers should just collapse.
        assert_eq!(225, hstack.bounds(&surface, bounds.zero_hinted()).width);
        hstack.draw(&mut surface, 0, 0, bounds.zero_hinted());
        assert!(register.was_drawn_at(1, 0, 0));
        assert!(register.was_drawn_at(2, 100, 0));
        assert!(register.was_drawn_at(3, 175, 0));
    }

    #[test]
    fn test_hstack_layouts_multiple_spacers_optimally_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 100, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 75, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(3, register.clone(), 50, 50)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Optimally-hinted, so let spacers expand up to max.
        assert_eq!(bounds.width, hstack.bounds(&surface, bounds.optimally_hinted()).width);
        hstack.draw(&mut surface, 0, 0, bounds.optimally_hinted());
        // The first one is just at the top. After that, we'd expect the first
        // and second spacers to take up equal height, so 87 and 88 pixels.
        // (400 - (100 + 75 + 50)) / 2 = (400 - 225) / 2 = 175 / 2 = 87.5
        assert!(register.was_drawn_at(1, 0, 0));
        assert!(register.was_drawn_at(2, 187, 0));
        assert!(register.was_drawn_at(3, 350, 0));
    }

    #[test]
    fn test_hstack_layouts_multiple_spacers_infinitely_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 100, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 75, 50)));
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(TestView::monitored(3, register.clone(), 50, 50)));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Infinitely-hinted, so let spacers expand up to max.
        assert_eq!(bounds.width, hstack.bounds(&surface, bounds.optimally_hinted()).width);
        hstack.draw(&mut surface, 0, 0, bounds.optimally_hinted());
        // The first one is just at the top. After that, we'd expect the first
        // and second spacers to take up equal height, so 87 and 88 pixels.
        // (400 - (100 + 75 + 50)) / 2 = (400 - 225) / 2 = 175 / 2 = 87.5
        assert!(register.was_drawn_at(1, 0, 0));
        assert!(register.was_drawn_at(2, 187, 0));
        assert!(register.was_drawn_at(3, 350, 0));
    }

    #[test]
    fn test_hstack_layouts_purely_spacers_zero_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(Spacer::horizontal()));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Zero-hinted, so the spacers should collapse to zero, which means the
        // entire hstack collapses to zero.
        assert_eq!(0, hstack.bounds(&surface, bounds.zero_hinted()).width);
    }

    #[test]
    fn test_hstack_layouts_purely_spacers_optimally_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(Spacer::horizontal()));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Optimally-hinted, so spacers expand to the maximum possible.
        assert_eq!(bounds.width, hstack.bounds(&surface, bounds.optimally_hinted()).width);
    }

    #[test]
    fn test_hstack_layouts_purely_spacers_infinitely_hinted() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(Spacer::horizontal()));
        hstack.views.push(Box::new(Spacer::horizontal()));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        // Infinitely-hinted, so spacers expand to the maximum possible.
        assert_eq!(bounds.width, hstack.bounds(&surface, bounds.infinitely_hinted()).width);
    }

    #[test]
    fn test_hstack_layouts_nested_hstack() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut inner_hstack = HStack::new();
        inner_hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 50)));
        inner_hstack.views.push(Box::new(Spacer::horizontal()));
        inner_hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 80, 80)));
        let mut outer_hstack = HStack::new();
        outer_hstack.views.push(Box::new(inner_hstack));
        outer_hstack.views.push(Box::new(TestView::monitored(3, register.clone(), 100, 100)));
        outer_hstack.views.push(Box::new(MonitorWrapper::new(4, register.clone(), Box::new(Spacer::horizontal()))));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        outer_hstack.draw(&mut surface, 0, 0, bounds.infinitely_hinted());
        // First, the least-flexible view — the test view 3 of height 100 — will
        // be offered 133 pixels, but it'll only need 100 pixels. Then, 150 of
        // the remaining 300 pixels will be offered to the inner hstack, since
        // that's _slightly_ less flexible than the bottom spacer. It'll gladly
        // take the entire 150 pixels, expanding its spacer to 20 pixels. After
        // that, the bottom spacer will get the remaining 150 pixels.
        assert!(register.was_drawn_at(1, 0, 0));
        assert!(register.was_drawn_at(2, 70, 0));
        assert!(register.was_drawn_at(3, 150, 0));
        assert!(register.was_drawn_at(4, 250, 0));
    }

    #[test]
    fn test_can_mix_hstack_and_vstack() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut top_hstack = HStack::new();
        top_hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 50, 30)));
        top_hstack.views.push(Box::new(Spacer::horizontal()));
        top_hstack.views.push(Box::new(TestView::monitored(2, register.clone(), 70, 35)));
        let mut bottom_hstack = HStack::new();
        bottom_hstack.views.push(Box::new(TestView::monitored(3, register.clone(), 50, 40)));
        bottom_hstack.views.push(Box::new(TestView::monitored(4, register.clone(), 70, 40)));
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(top_hstack));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(TestView::monitored(5, register.clone(), 150, 100)));
        vstack.views.push(Box::new(Spacer::vertical()));
        vstack.views.push(Box::new(bottom_hstack));
        let bounds = surface.bounds() - Bounds::new(100, 100);
        vstack.draw(&mut surface, 0, 0, bounds.infinitely_hinted());
        // The first view should be in the top left corner.
        assert!(register.was_drawn_at(1, 0, 0));
        // The second view in the top right corner.
        assert!(register.was_drawn_at(2, 400 - 70, 0));
        // The third view is in the bottom left corner. The fourth one follows
        // immediately.
        assert!(register.was_drawn_at(3, 0, 400 - 40));
        assert!(register.was_drawn_at(4, 50, 400 - 40));
        // Number five is somewhere in the middle. We have the top hstack taking
        // up 35 pixels. And the bottom hstack taking up 40 pixels. So that
        // leaves 400 - (35 + 40) = 325 pixels. View number five itself takes up
        // 100 pixels, so we have 225 pixels to distribute among the two
        // vertical spacers. We size them equally, so 112 pixels for the first
        // and 113 for the second.
        assert!(register.was_drawn_at(5, 0, 35 + 112));
    }

    #[test]
    fn test_can_set_padding_on_view() {
        let mut view = TestView::new(300, 300);
        assert_eq!(0, view.padding.left);
        assert_eq!(0, view.padding.right);
        assert_eq!(0, view.padding.top);
        assert_eq!(0, view.padding.bottom);
        view.padding(Edge::Left, 1);
        view.padding(Edge::Right, 2);
        view.padding(Edge::Top, 3);
        view.padding(Edge::Bottom, 4);
        assert_eq!(1, view.padding.left);
        assert_eq!(2, view.padding.right);
        assert_eq!(3, view.padding.top);
        assert_eq!(4, view.padding.bottom);
    }

    #[test]
    fn test_vstack_renders_top_and_left_padding() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut vstack = VStack::new();
        vstack.views.push(Box::new(TestView::monitored(1, register.clone(), 150, 100)));
        vstack.padding(Edge::Left, 10);
        vstack.padding(Edge::Right, 5);
        vstack.padding(Edge::Top, 15);
        vstack.padding(Edge::Bottom, 2);
        let bounds = surface.bounds() - Bounds::new(100, 100);
        assert_eq!(150 + 15, vstack.bounds(&surface, bounds.zero_hinted()).width);
        assert_eq!(150 + 15, vstack.bounds(&surface, bounds.optimally_hinted()).width);
        assert_eq!(150 + 15, vstack.bounds(&surface, bounds.infinitely_hinted()).width);
        assert_eq!(100 + 17, vstack.bounds(&surface, bounds.zero_hinted()).height);
        assert_eq!(100 + 17, vstack.bounds(&surface, bounds.optimally_hinted()).height);
        assert_eq!(100 + 17, vstack.bounds(&surface, bounds.infinitely_hinted()).height);
        vstack.draw(&mut surface, 0, 0, bounds.infinitely_hinted());
        assert!(register.was_drawn_at(1, 10, 15));
    }

    #[test]
    fn test_vstack_padding_cuts_into_space_for_views() {
        // When the VStack includes a spacer like this:
        //
        // +-------------------+
        // |    Test View      |
        // |   (100 x 150)     |
        // +-------------------+
        // |      Spacer       |
        // +-------------------+
        // |    Test View      |
        // |    (75 x 75)      |
        // +-------------------+
        //
        // Since this is a VStack, let's think about the height of the thing
        // first. Let's say that we have 200 px of height for the entire thing.
        // Then in all three hinting modes, the VStack will take up 225 px.
        // Since the test views are not willing to flex and they need a minimum
        // of 225 px, no matter the suggested bounds for the view.
        //
        // Padding is also not flexible. So, in this situation, the height of
        // the VStack would increase further by the top and bottom padding, no
        // matter the suggested bounds.
        //
        // If we expand the suggested bounds to _just_ cover the required height
        // of the views plus the padding -- let's say that we pad by 10 px at
        // the top and by 5 px at the bottom, so 240 px in total -- then the
        // spacer would always collapse to zero. Everything beyond 240 px would
        // be left over for the spacer in infinitely- and optimally-hinted modes.
        //
        // As for the horizontal extend of the VStack: that part is reasonably
        // easy: the VStack just adds its own padding to the total width
        // required.
        //
        // To test this, we'll embed a VStack in another VStack, like this:
        //
        // +--------------------------+
        // |        Test View         |
        // +--------------------------+
        // |    VStack w/ padding     |
        // | +----------------------+ |
        // | |      Test View       | |
        // | +----------------------+ |
        // | |        Spacer        | |
        // | +----------------------+ |
        // | |      Test View       | |
        // | +----------------------+ |
        // +--------------------------+
        // |        Test View         |
        // +--------------------------+
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut inner_stack = VStack::new();
        inner_stack.views.push(Box::new(TestView::monitored(2, register.clone(), 100, 150)));
        inner_stack.views.push(Box::new(Spacer::vertical()));
        inner_stack.views.push(Box::new(TestView::monitored(3, register.clone(), 75, 75)));
        inner_stack.padding(Edge::Top, 10);
        inner_stack.padding(Edge::Bottom, 5);
        inner_stack.padding(Edge::Left, 15);
        inner_stack.padding(Edge::Right, 2);
        let mut outer_stack = VStack::new();
        outer_stack.views.push(Box::new(TestView::monitored(1, register.clone(), 100, 40)));
        outer_stack.views.push(Box::new(inner_stack));
        outer_stack.views.push(Box::new(TestView::monitored(4, register.clone(), 80, 50)));
        // Give the entire outer stack 350 px of height. That should leave
        // 350 - 150 - 75 - 40 - 50 - 10 - 5 = 20 px for the spacer.
        let bounds = surface.bounds() - Bounds::new(150, 150);
        outer_stack.draw(&mut surface, 0, 0, bounds.optimally_hinted());
        assert!(register.was_drawn_at(1, 0, 0));
        // 15 (left padding) x 40 + 10 (top padding)
        assert!(register.was_drawn_at(2, 15, 50));
        // Spacer in between 2 and 3.
        // 15 (left padding) x 40 + 20 (spacer) + 150 + 10 (top padding)
        assert!(register.was_drawn_at(3, 15, 220));
        // 0 x 40 + 20 (spacer) + 150 + 75 + 10 (top padding) + 5 (bottom padding)
        assert!(register.was_drawn_at(4, 0, 300));
    }

    #[test]
    fn test_hstack_renders_top_and_left_padding() {
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut hstack = HStack::new();
        hstack.views.push(Box::new(TestView::monitored(1, register.clone(), 150, 100)));
        hstack.padding(Edge::Left, 10);
        hstack.padding(Edge::Right, 5);
        hstack.padding(Edge::Top, 15);
        hstack.padding(Edge::Bottom, 2);
        let bounds = surface.bounds() - Bounds::new(100, 100);
        assert_eq!(150 + 15, hstack.bounds(&surface, bounds.zero_hinted()).width);
        assert_eq!(150 + 15, hstack.bounds(&surface, bounds.optimally_hinted()).width);
        assert_eq!(150 + 15, hstack.bounds(&surface, bounds.infinitely_hinted()).width);
        assert_eq!(100 + 17, hstack.bounds(&surface, bounds.zero_hinted()).height);
        assert_eq!(100 + 17, hstack.bounds(&surface, bounds.optimally_hinted()).height);
        assert_eq!(100 + 17, hstack.bounds(&surface, bounds.infinitely_hinted()).height);
        hstack.draw(&mut surface, 0, 0, bounds.infinitely_hinted());
        assert!(register.was_drawn_at(1, 10, 15));
    }

    #[test]
    fn test_hstack_padding_cuts_into_space_for_views() {
        // The argument for the padding goes pretty much exactly like in the
        // VStack test. We use the following test setup:
        //
        // +--------+---------------------------------+--------+
        // |        |       HStack w/ padding         |        |
        // |  Test  | +--------+----------+--------+  |  Test  |
        // |  View  | |  Test  |  Spacer  |  Test  |  |  View  |
        // |        | |  View  |          |  View  |  |        |
        // |        | +--------+----------+--------+  |        |
        // +--------+---------------------------------+--------+
        let mut surface = Surface::new(500, 500).unwrap();
        let mut register = Rc::new(DrawingRegister::new());
        let mut inner_stack = HStack::new();
        inner_stack.views.push(Box::new(TestView::monitored(2, register.clone(), 150, 100)));
        inner_stack.views.push(Box::new(Spacer::horizontal()));
        inner_stack.views.push(Box::new(TestView::monitored(3, register.clone(), 75, 75)));
        inner_stack.padding(Edge::Left, 10);
        inner_stack.padding(Edge::Right, 5);
        inner_stack.padding(Edge::Top, 15);
        inner_stack.padding(Edge::Bottom, 2);
        let mut outer_stack = HStack::new();
        outer_stack.views.push(Box::new(TestView::monitored(1, register.clone(), 40, 100)));
        outer_stack.views.push(Box::new(inner_stack));
        outer_stack.views.push(Box::new(TestView::monitored(4, register.clone(), 50, 80)));
        // Give the entire outer stack 350 px of width. That should leave
        // 350 - 150 - 75 - 40 - 50 - 10 - 5 = 20 px for the spacer.
        let bounds = surface.bounds() - Bounds::new(150, 150);
        outer_stack.draw(&mut surface, 0, 0, bounds.optimally_hinted());
        assert!(register.was_drawn_at(1, 0, 0));
        // 40 + 10 (left padding) x 15 (top padding)
        assert!(register.was_drawn_at(2, 50, 15));
        // Spacer in between 2 and 3.
        // 40 + 20 (spacer) + 150 + 10 (left padding) x 15 (top padding)
        assert!(register.was_drawn_at(3, 220, 15));
        // 40 + 20 (spacer) + 150 + 75 + 10 (left padding) + 5 (right padding) x 0
        assert!(register.was_drawn_at(4, 300, 0));
    }
}
