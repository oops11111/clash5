fn main() {
    std::thread::Builder::new()
        .name("tauri-command-contract".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            app_lib::export_frontend_command_contract().expect("export frontend Tauri command contract");
        })
        .expect("spawn command contract exporter")
        .join()
        .expect("command contract exporter thread panicked");
}
