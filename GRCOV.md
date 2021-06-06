# Getting test coverage report with grcov #

Get [grcov](https://github.com/mozilla/grcov ) and install their requirements for your system.

To get a complete test report run the following commands with these env
variables set. (I found some flags they suggested broke with some of our packages.)

`.bashrc`
```shell
export LLVM_PROFILE_FILE="your_name-%p-%m.profraw"
export RUSTDOCFLAGS="-Cpanic=abort"
```

1. `RUSTFLAGS="-Zinstrument-coverage -Ccodegen-units=1 -Copt-level=0 -Coverflow-checks=off -Zpanic_abort_tests" CARGO_INCREMENTAL=0 RUSTC_BOOTSTRAP=1 cargo build`
1. `RUSTFLAGS="-Zinstrument-coverage -Ccodegen-units=1 -Copt-level=0 -Coverflow-checks=off -Zpanic_abort_tests" CARGO_INCREMENTAL=0 RUSTC_BOOTSTRAP=1 cargo test`
1. `grcov . -s . --binary-path ./target/debug/ -t html --branch --ignore-not-existing -o ./target/debug/coverage/`


## Known issues ##

- If you run `grcov` on Windows you will need to run it as admin or it will fail
with an error related to symlinks. [Issue 561](https://github.com/mozilla/grcov/issues/561)
- `[ERROR] Execution count overflow detected.` can occur when running `grcov`
  but is not fatal and a report will still be created. [Issue 613](https://github.com/mozilla/grcov/issues/613)
