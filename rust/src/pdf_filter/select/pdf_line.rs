pub mod font;
pub mod text;
pub mod area;
pub mod font_size;

use font::FontSet;
use text::TextSet;
use area::Area;
use font_size::FontSizeInterval;

pub struct PdfLineSet{
    font: FontSet,
    font_size: FontSizeInterval,
    text: TextSet,
    area: Area
}