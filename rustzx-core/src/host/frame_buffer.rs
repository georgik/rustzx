use crate::zx::video::colors::{ZXBrightness, ZXColor};

pub enum FrameBufferSource {
    Screen,
    Border,
}

pub trait FrameBuffer {
    type Context: Clone;
    /// Creates canvas size with required dimensions (`width`, `height`)
    fn new(width: usize, height: usize, source: FrameBufferSource, context: Self::Context) -> Self;
    /// Set `color` with `brightness` for pixel on canvas at (`x`, `y`)
    fn set_color(&mut self, x: usize, y: usize, color: ZXColor, brightness: ZXBrightness);
    fn set_colors(&mut self, x: usize, y: usize, colors: [ZXColor; 8], brightness: ZXBrightness);
    fn reset_dirty_regions(&mut self);
}
