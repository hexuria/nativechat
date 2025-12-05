use gpui::actions;

actions!(
    nativechat,
    [
        ToggleSidebar,
        ToggleTheme,
        OpenSettings,
        Minimize,
        Zoom,
        Hide,
        HideOthers,
        ShowAll,
        About,
        Quit,
        SelectProfile1,
        SelectProfile2,
        SelectAppCanva,
        SelectAppFigma,
        SelectAppNotion,
        SelectAppLinear,
        SelectAppPhotos,
        SelectAppWebSearch,
        SelectAppCanvas,
        SelectAppCoursera,
        SelectAppSpotify,
        SelectAppImageGeneration,
        SelectAppThinking,
        SelectAppDeepResearch,
        SelectAppStudy,
        NewChat,
        Search,
        Library,
        Projects,
        OpenAccountSettings,
        OpenProfileSettings,
        SignOut,
        ToggleCredentialsModal,
        ToggleDebugMarkdown
    ]
);

use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize)]
pub struct SelectSession {
    pub id: String,
}

impl gpui::Action for SelectSession {
    fn name(&self) -> &'static str {
        "SelectSession"
    }
    fn name_for_type() -> &'static str {
        "SelectSession"
    }
    fn boxed_clone(&self) -> Box<dyn gpui::Action> {
        Box::new(self.clone())
    }
    fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .map_or(false, |s| self == s)
    }
    fn build(value: serde_json::Value) -> gpui::Result<Box<dyn gpui::Action>> {
        Ok(Box::new(serde_json::from_value::<Self>(value)?))
    }
}

#[derive(Clone, PartialEq, Deserialize)]
pub struct StartRenameSession {
    pub id: String,
    pub title: String,
}

impl gpui::Action for StartRenameSession {
    fn name(&self) -> &'static str {
        "StartRenameSession"
    }
    fn name_for_type() -> &'static str {
        "StartRenameSession"
    }
    fn boxed_clone(&self) -> Box<dyn gpui::Action> {
        Box::new(self.clone())
    }
    fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .map_or(false, |s| self == s)
    }
    fn build(value: serde_json::Value) -> gpui::Result<Box<dyn gpui::Action>> {
        Ok(Box::new(serde_json::from_value::<Self>(value)?))
    }
}

#[derive(Clone, PartialEq, Deserialize)]
pub struct DeleteSession {
    pub id: String,
}

impl gpui::Action for DeleteSession {
    fn name(&self) -> &'static str {
        "DeleteSession"
    }
    fn name_for_type() -> &'static str {
        "DeleteSession"
    }
    fn boxed_clone(&self) -> Box<dyn gpui::Action> {
        Box::new(self.clone())
    }
    fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .map_or(false, |s| self == s)
    }
    fn build(value: serde_json::Value) -> gpui::Result<Box<dyn gpui::Action>> {
        Ok(Box::new(serde_json::from_value::<Self>(value)?))
    }
}

actions!(
    sidebar,
    [
        ConfirmDeleteSession,
        CancelDeleteSession,
        SubmitRenameSession,
        CancelRenameSession
    ]
);
