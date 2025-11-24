use gpui::{AssetSource, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
pub struct LocalAssets;

pub struct CombinedAssets;

impl AssetSource for CombinedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>, anyhow::Error> {
        // Try local assets first
        if let Some(f) = LocalAssets::get(path) {
            return Ok(Some(f.data));
        }
        // Fallback to default assets
        ui::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>, anyhow::Error> {
        let mut files = ui::assets::Assets.list(path)?;
        for file in LocalAssets::iter() {
            if file.starts_with(path) {
                files.push(file.to_string().into());
            }
        }
        Ok(files)
    }
}
