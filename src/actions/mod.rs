mod install;
mod new;

pub use install::install;
pub use new::new;


pub const NEW_ERROR_MESSAGE: &str = indoc::indoc! {r#"
Problem creating the project. This may be due to a permissions problem.
If on linux, please try again with `sudo`.
"#};