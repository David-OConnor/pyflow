use std::{
    collections::HashMap,
    io::{self, Write},
};

use termcolor::Color;

use crate::{
    dep_types::Version,
    util::{abort, default_python, fallible_v_parse, print_color},
};

/// Ask the user what Python version to use.
pub fn py_vers() -> Version {
    print_color(
        "Please enter the Python version for this project: (eg: 3.8)",
        Color::Magenta,
    );
    let default_ver = default_python();
    print!("Default [{}]:", default_ver);
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Unable to read user input for version");

    input.pop(); // Remove trailing newline.
    let input = input.replace("\n", "").replace("\r", "");
    if !input.is_empty() {
        fallible_v_parse(&input)
    } else {
        default_ver
    }
}

/// A generic prompt function, where the user selects from a list
pub fn list<T: Clone + ToString>(
    init_msg: &str,
    type_: &str,
    items: &[(String, T)],
    show_item: bool,
) -> (String, T) {
    print_color(init_msg, Color::Magenta);
    for (i, (name, content)) in items.iter().enumerate() {
        if show_item {
            println!("{}: {}: {}", i + 1, name, content.to_string())
        } else {
            println!("{}: {}", i + 1, name)
        }
    }

    let mut mapping = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        mapping.insert(i + 1, item);
    }

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Problem reading input");

    let input = input
        .chars()
        .next()
        .expect("Problem parsing input")
        .to_string()
        .parse::<usize>();

    let input = if let Ok(ip) = input {
        ip
    } else {
        abort("Please try again; enter a number like 1 or 2 .");
        unreachable!()
    };

    let (name, content) = if let Some(r) = mapping.get(&input) {
        r
    } else {
        abort(&format!(
            "Can't find the {} associated with that number. Is it in the list above?",
            type_
        ));
        unreachable!()
    };

    (name.to_string(), content.clone())
}
