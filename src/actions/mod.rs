mod clear;
mod init;
mod install;
mod list;
mod new;
mod package;

pub use clear::clear;
pub use init::init;
pub use install::install;
pub use list::list;
pub use new::new;
pub use package::package;

pub const NEW_ERROR_MESSAGE: &str = indoc::indoc! {r#"
Problem creating the project. This may be due to a permissions problem.
If on linux, please try again with `sudo`.
"#};
