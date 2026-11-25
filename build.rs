fn main() {
    // тёмная тема стандартных элементов (флажки, индикатор, список)
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/main.slint", config).expect("не удалось скомпилировать ui/main.slint");

    // иконка и сведения о программе в .exe (только при сборке на Windows)
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/Blockbuster.ico");
        res.set("ProductName", "Blockbuster");
        res.set("FileDescription", "Blockbuster");
        res.set("InternalName", "Blockbuster");
        res.set("OriginalFilename", "Blockbuster.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=иконка не встроена: {}", e);
        }
    }
}
