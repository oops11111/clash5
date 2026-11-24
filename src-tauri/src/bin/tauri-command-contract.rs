fn main() -> Result<(), Box<dyn std::error::Error>> {
    let exporter = std::thread::Builder::new()
        .name("tauri-command-contract".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| app_lib::export_frontend_command_contract().map_err(|error| error.to_string()))?;

    match exporter.join() {
        Ok(result) => result.map_err(std::io::Error::other)?,
        Err(_) => return Err(std::io::Error::other("command contract exporter thread panicked").into()),
    }

    Ok(())
}
