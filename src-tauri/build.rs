fn main() {
    println!("cargo:rerun-if-changed=migrations");
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
    let probe = destination.join(format!("ffprobe-{target}{extension}"));
    for file in [
        &binary,
        &probe,
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
    println!("cargo:rustc-env=PIXOMA_FFPROBE_PATH={}", probe.display());
    println!("cargo:rerun-if-changed=../apps/edge-agent");
    let root = directory.parent().expect("Studio 工程目录不存在");
    let cache = root.join(".local-cache");
    std::fs::create_dir_all(cache.join("go-tmp")).expect("创建 Go 临时目录失败");
    std::fs::create_dir_all(cache.join("go-build")).expect("创建 Go 构建缓存失败");
    let agent = destination.join(format!("edge-agent-{target}{extension}"));
    let status = std::process::Command::new("go")
        .current_dir(root.join("apps/edge-agent"))
        .env("TMPDIR", cache.join("go-tmp"))
        .env("GOTMPDIR", cache.join("go-tmp"))
        .env("GOCACHE", cache.join("go-build"))
        .args(["build", "-trimpath", "-o"])
        .arg(&agent)
        .arg("./cmd/edge-agent")
        .status().expect("启动 Go Agent 构建失败");
    assert!(status.success(), "构建 Go Agent 失败");
    println!("cargo:rustc-env=PIXOMA_EDGE_AGENT_PATH={}", agent.display());
    tauri_build::build();
}
