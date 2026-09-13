use gpui_kit::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
#[include = "**/*.png"]
pub struct LocalAssets;

pub struct CombinedAssets;

impl AssetSource for CombinedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(f) = LocalAssets::get(path) {
            return Ok(Some(f.data));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut files = gpui_kit::assets::Assets.list(path).unwrap_or_default();
        for file in LocalAssets::iter() {
            if file.starts_with(path) {
                let name = SharedString::from(file.to_string());
                if !files.contains(&name) {
                    files.push(name);
                }
            }
        }
        Ok(files)
    }
}
