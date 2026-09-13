use gpui_kit::assets::IconNamed;
use gpui_kit::SharedString;

/// App-specific icons that are not in the GPUI Kit Lucide catalog.
///
/// Paths resolve through [`crate::assets::CombinedAssets`], which prefers
/// `assets/icons` over the kit default bundle.
#[derive(Clone, Copy)]
pub enum NativeIcon {
    Branch,
    Canvas,
    Canva,
    Clip,
    Close,
    Collections,
    Figma,
    Coursera,
    CreateImage,
    DeepSearch,
    Pencil,
    Plugins,
    ReadAloud,
    Report,
    Session,
    Spotify,
    Study,
    Thinking,
    Trash,
    WebSearch,
    WizardHat,
}

impl IconNamed for NativeIcon {
    fn path(self) -> SharedString {
        match self {
            Self::Branch => "icons/branch.svg",
            Self::Canvas => "icons/canvas.svg",
            Self::Canva => "icons/canva.svg",
            Self::Clip => "icons/clip.svg",
            Self::Close => "icons/close.svg",
            Self::Collections => "icons/collections.svg",
            Self::Coursera => "icons/coursera.svg",
            Self::Figma => "icons/figma.svg",
            Self::CreateImage => "icons/create_image.svg",
            Self::DeepSearch => "icons/deep_search.svg",
            Self::Pencil => "icons/pencil.svg",
            Self::Plugins => "icons/plugins.svg",
            Self::ReadAloud => "icons/read-aloud.svg",
            Self::Report => "icons/report.svg",
            Self::Session => "icons/session.svg",
            Self::Spotify => "icons/spotify.svg",
            Self::Study => "icons/study.svg",
            Self::Thinking => "icons/thinking.svg",
            Self::Trash => "icons/trash.svg",
            Self::WebSearch => "icons/web_search.svg",
            Self::WizardHat => "icons/wizard_hat.svg",
        }
        .into()
    }
}
