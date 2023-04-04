fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS") {
        Ok(os) if os == "windows" => {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("qtts.ico");
            res.compile().unwrap();
        }
        Ok(_) => {
            eprintln!("Warning: Target OS must be 'windows' in order for this application to work!")
        }
        Err(e) => eprintln!("Failed to set WindowsRessources: {}", e),
    }
}
