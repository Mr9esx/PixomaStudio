fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-env-changed=PIXOMA_PLATFORM_URL");
    if std::env::var("PROFILE").expect("构建模式不存在") == "release" {
        let address =
            std::env::var("PIXOMA_PLATFORM_URL").expect("构建安装包需要设置 PIXOMA_PLATFORM_URL");
        assert!(!address.trim().is_empty(), "安装包的平台地址不能为空");
    }
    let target = std::env::var("TARGET").expect("构建目标不存在");
    assert_eq!(
        target,
        std::env::var("HOST").expect("构建主机不存在"),
        "视频解码程序需要在对应系统构建"
    );
    let directory =
        std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("工程目录不存在"));
    let destination = directory.join("binaries");
    let extension = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let binary = destination.join(format!("ffmpeg-{target}{extension}"));
    for file in [
        &binary,
        &destination.join("FFmpeg-LICENSE.txt"),
        &destination.join("FFmpeg-README.txt"),
        &destination.join("FFmpeg-source.tar.xz"),
    ] {
        println!("cargo:rerun-if-changed={}", file.display());
        assert!(
            file.is_file(),
            "视频解码程序或说明缺失，请运行 pnpm prepare:media"
        );
    }
    let status = std::process::Command::new(&binary)
        .arg("-version")
        .output()
        .expect("无法启动视频解码程序");
    assert!(status.status.success(), "视频解码程序检查失败");
    println!("cargo:rustc-env=PIXOMA_FFMPEG_PATH={}", binary.display());
    tauri_build::build();
}
