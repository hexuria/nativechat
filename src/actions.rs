use gpui_kit::Action;
use gpui_kit::actions;
use schemars::JsonSchema;
use serde::Deserialize;

actions!(
    nativechat,
    [
        ToggleSidebar,
        ToggleMiniSidebar,
        ToggleAgentSettings,
        ToggleComputerPane,
        NavBack,
        NavForward,
        ToggleTheme,
        OpenSettings,
        CloseSettings,
        Minimize,
        Zoom,
        Hide,
        HideOthers,
        ShowAll,
        About,
        Quit,
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
        FindNext,
        FindPrev,
        CloseFind,
        FocusChatInput,
        OpenCommandPalette,
        CloseCommandPalette,
        PaletteNextTab,
        PalettePrevTab,
        PaletteSelectNext,
        PaletteSelectPrev,
        CloseBotFinder,
        ClearSearch,
        Library,
        Projects,
        OpenAccountSettings,
        SignOut,
        ToggleDebugMarkdown,
        ToggleFps,
        CopyMessage,
        BranchInNewChat,
        ReportMessage,
        PauseReadAloud,
        ResumeReadAloud,
        StopReadAloud
    ]
);

#[derive(Clone, PartialEq, Debug, Deserialize, Default, JsonSchema)]
pub enum TtsSource {
    #[default]
    Native,
}

#[derive(Clone, PartialEq, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = nativechat)]
pub struct PickFinderItem {
    pub index: usize,
}

#[derive(Clone, PartialEq, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = nativechat)]
pub struct SelectSession {
    pub id: String,
}

#[derive(Clone, PartialEq, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = nativechat)]
pub struct StartRenameSession {
    pub id: String,
    pub title: String,
}

#[derive(Clone, PartialEq, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = nativechat)]
pub struct DeleteSession {
    pub id: String,
}

#[derive(Clone, PartialEq, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = nativechat)]
pub struct ReadAloud {
    pub text: String,
    pub message_id: String,
}

#[derive(Clone, PartialEq, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = nativechat)]
pub struct ToggleReadAloud {
    pub text: String,
    pub message_id: String,
    pub mode: TtsSource,
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
