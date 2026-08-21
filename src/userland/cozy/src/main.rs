#[cfg(not(test))]
fn main() -> std::io::Result<()> {
    cozy::run_cli_from_env()
}

#[cfg(test)]
fn main() {}
