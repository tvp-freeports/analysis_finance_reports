pub mod font;
pub mod text;
pub mod area;
pub mod font_size;

use font::FontSet;
use text::TextSet;
use area::Area;
use font_size::FontSizeInterval;

pub struct PdfLineSet{
    font: Option<FontSet>,
    font_size: Option<FontSizeInterval>,
    text: Option<TextSet>,
    area: Option<Area>
}