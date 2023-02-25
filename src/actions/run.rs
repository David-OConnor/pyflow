use crate::{commands, pyproject::Config, util::abort};
use regex::Regex;
use std::path::Path;

/// Execute a python CLI tool, either specified in `pyproject.toml`, or in a dependency.
pub fn run(lib_path: &Path, bin_path: &Path, vers_path: &Path, cfg: &Config, args: Vec<String>) {
    // Allow both `pyflow run ipython` (args), and `pyflow ipython` (opt.script)
    if args.is_empty() {
        return;
    }

    let name = if let Some(a) = args.get(0) {
        a.clone()
    } else {
        abort("`run` must be followed by the script to run, eg `pyflow run black`");
    };

    // If the script we're calling is specified in `pyproject.toml`, ensure it exists.

    // todo: Delete these scripts as required to sync with pyproject.toml.
    let re = Regex::new(r"(.*?):(.*)").unwrap();

    let mut specified_args: Vec<String> = args.into_iter().skip(1).collect();

    // If a script name is specified by by this project and a dependency, favor
    // this project.
    if let Some(s) = cfg.scripts.get(&name) {
        let abort_msg = format!(
            "Problem running the function {}, specified in `pyproject.toml`",
            name,
        );

        if let Some(caps) = re.captures(s) {
            let module = caps.get(1).unwrap().as_str();
            let function = caps.get(2).unwrap().as_str();
            let mut args_to_pass = vec![
                "-c".to_owned(),
                format!(r#"import {}; {}.{}()"#, module, module, function),
            ];

            args_to_pass.append(&mut specified_args);
            if commands::run_python(bin_path, &[lib_path.to_owned()], &args_to_pass).is_err() {
                abort(&abort_msg);
            }
        } else {
            abort(&format!("Problem parsing the following script: {:#?}. Must be in the format module:function_name", s));
        }
        return;
    }
    //            None => {
    let abort_msg = format!(
        "Problem running the CLI tool {}. Is it installed? \
         Try running `pyflow install {}`",
        name, name
    );
    let script_path = vers_path.join("bin").join(name);
    if !script_path.exists() {
        abort(&abort_msg);
    }

    let mut args_to_pass = vec![script_path
        .to_str()
        .expect("Can't find script path")
        .to_owned()];

    args_to_pass.append(&mut specified_args);
    if commands::run_python(bin_path, &[lib_path.to_owned()], &args_to_pass).is_err() {
        abort(&abort_msg);
    }
}
