use clap::ValueEnum;
use crate::draw::Surface;
use anyhow::Result;

/// This enum contains all modules. The options for the `module` CLI parameter 
/// are built from this.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Module {
    /// Show the latest headline from one of a set of major German news outlets.
    NewsHeadlines,
}

/// Options returned by a module besides drawing the actual image onto the
/// surface.
pub struct ViewOptions {
    /// The time-to-live for the generated image, in minutes.
    pub ttl: Option<u32>,
}

impl ViewOptions {
    fn none() -> Self {
        ViewOptions{
            ttl: None,
        }
    }
}
 
/// This is the trait that every module needs to implement: just one method that
/// takes the surface to draw on and returns a result. The method should use the
/// surface's bounds to adapt it's drawing to the particular display (or error
/// out if it's not capable of supporting that display).
pub trait InfoView {
    fn generate(&self, surface: &mut Surface) -> Result<ViewOptions>;
}
