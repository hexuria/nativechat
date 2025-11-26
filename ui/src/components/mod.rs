// Components - actual source code

pub mod avatar;
pub mod button;
pub mod icon;
pub mod input;
pub mod label;
pub mod popover;
pub mod resizable;
pub mod sidebar;
pub mod tab;
pub mod tooltip;

// Re-export main types
pub use avatar::Avatar;
pub use button::Button;
pub use icon::{Icon, IconName};
pub use input::{Input, InputEvent, InputState};
pub use label::Label;
pub use popover::Popover;
pub use resizable::{h_resizable, resizable_panel, v_resizable};
pub use sidebar::{
    Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
    SidebarToggleButton,
};
pub use tab::{Tab, TabBar, TabVariant};
pub use tooltip::Tooltip;

pub fn init(cx: &mut gpui::App) {
    input::init(cx);
}
