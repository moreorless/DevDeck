fn main() {
    // exe 图标资源在构建脚本中嵌入；图标变化时需要重新运行，否则增量构建沿用旧图标
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
