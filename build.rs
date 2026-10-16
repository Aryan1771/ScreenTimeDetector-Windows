fn main() {
    slint_build::compile("ui/main.slint").expect("failed to compile Slint UI");
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/logo.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/logo.ico")
            .set("ProductName", "Digital Wellbeing")
            .set("FileDescription", "Digital Wellbeing")
            .compile()
            .expect("failed to embed Windows application icon");
    }
}
