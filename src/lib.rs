//! rg52-remote: liga e desliga os "Remote Services" do dArkOS (SSH, Samba e filebrowser)
//! numa tela unica, no mesmo visual dos outros apps rg52-*.
pub mod devices;
pub mod gfx;
pub mod header;
pub mod remote;
pub mod ui;

pub use remote::{Level, Svcs};
