use gpui_kit::actions;
use gpui_kit::Action;
use schemars::JsonSchema;
use serde::Deserialize;

actions!(
    nativechat,
    [
        ToggleSidebar,
        ToggleMiniSidebar,
        ToggleAgentSettings,
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
    AI,
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

#[derive(Clone, PartialEq, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = nativechat)]
pub struct RegenerateAudio {
    pub text: String,
    pub message_id: String,
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
