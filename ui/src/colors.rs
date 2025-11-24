// Color functions stub
// These are referenced by various components but don't actually exist in gpui-component
// Creating stub implementations that return reasonable defaults

use gpui::Hsla;

pub fn white() -> Hsla {
    gpui::white()
}

pub fn black() -> Hsla {
    gpui::black()
}

pub fn red_400() -> Hsla {
    gpui::hsla(0.0, 0.9, 0.6, 1.0)
}

pub fn red_500() -> Hsla {
    gpui::hsla(0.0, 0.84, 0.55, 1.0)
}

pub fn red_600() -> Hsla {
    gpui::hsla(0.0, 0.78, 0.5, 1.0)
}

pub fn red_800() -> Hsla {
    gpui::hsla(0.0, 0.65, 0.4, 1.0)
}

pub fn blue_500() -> Hsla {
    gpui::hsla(0.6, 0.9, 0.55, 1.0)
}

pub fn yellow_500() -> Hsla {
    gpui::hsla(0.15, 0.9, 0.55, 1.0)
}

pub fn green_500() -> Hsla {
    gpui::hsla(0.33, 0.7, 0.5, 1.0)
}

pub fn pink_500() -> Hsla {
    gpui::hsla(0.9, 0.7, 0.65, 1.0)
}

pub fn stone_200() -> Hsla {
    gpui::hsla(0.0, 0.0, 0.9, 1.0)
}

pub fn stone_700() -> Hsla {
    gpui::hsla(0.0, 0.0, 0.3, 1.0)
}
