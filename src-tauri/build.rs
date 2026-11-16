fn main() {
    // ---------------------------------------------------------------------------
    // 防复发护栏：拦截「在 src-tauri/ 里直接 cargo build」这一错误构建方式。
    //
    // 背景（2026-11-16 白屏故障根因，已定位到 Tauri 源码级）：
    //   Tauri 2 的资产嵌入分支见 tauri-codegen-2.7.4/src/context.rs:178-195：
    //
    //       } else if dev && config.build.dev_url.is_some() {
    //           let assets = EmbeddedAssets::default();   // ← 零资源嵌入
    //       } else {
    //           let assets = ... frontend_dist ...        // ← 嵌入 dist/
    //       }
    //
    //   其中 dev 来自 tauri-2.12.5/build.rs:252-257：
    //
    //       let custom_protocol = has_feature(&mut checked_features, "custom-protocol");
    //       let dev = !custom_protocol;
    //       println!("cargo:dev={dev}");
    //
    //   即：只有启用了 tauri crate 的 custom-protocol feature，才是 production 构建，
    //   才会把 frontendDist 目录嵌进二进制。
    //
    //   Tauri CLI（npm run tauri build）会自动加上 --features tauri/custom-protocol；
    //   而在 src-tauri/ 里裸跑 `cargo build` 不带该 feature -> dev=true ->
    //   走上面第一个分支 -> 产出**不含任何前端资源**的 exe ->
    //   运行时只会去连 devUrl(http://localhost:1420)，Vite 没开就是整窗纯白。
    //
    //   本次故障的实测证据：
    //     旧 exe(裸 cargo build)：前端资源标记命中数 0，窗口纯白；
    //     新 exe(tauri build)   ：命中数 1，Rust 侧 gateway_status 探针确认 Vue 已挂载。
    //
    // 护栏行为：
    //   dev 构建（裸 cargo build）-> 打印 cargo:warning，明确告知「本 exe 无前端资源」。
    //   非 dev 构建但 dist/ 缺失 -> 直接 panic，把错误挡在编译期，
    //   避免再产出「能启动但整窗纯白」的可执行文件。
    // ---------------------------------------------------------------------------
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dist_dir = std::path::Path::new(&manifest_dir).join("../dist");

    // 必须先调用 tauri_build::build()，它会设置 DEP_TAURI_DEV，is_dev() 才可用。
    tauri_build::build();

    let is_dev = tauri_build::is_dev();
    println!("cargo:rerun-if-changed=../dist");

    let index_html = dist_dir.join("index.html");
    let has_assets = dist_dir.is_dir()
        && index_html.exists()
        && std::fs::read_dir(&dist_dir)
            .map(|mut it| it.any(|e| e.is_ok()))
            .unwrap_or(false);

    if is_dev {
        // 裸 cargo build：dev=true，本 exe 不含前端资源，运行时依赖 devUrl。
        // 允许通过（tauri dev 走的就是这条路径），但必须给出醒目告警。
        //
        // 注意：cargo:warning 只在 build script 被重新编译的那一次输出；
        // 缓存命中时 cargo 不会重跑 build script，告警就消失了。所以这里同时
        // 用 eprintln! 直接写 stderr —— build script 的 stderr 在 cargo 构建
        // 输出里始终可见（除非被 -q 抑制），保证每次构建都能看到提醒。
        const DEV_WARNING: &str = "\
[dou] ======================== 警告 ========================
[dou] 这是 dev 构建（未启用 custom-protocol），产物**不包含前端资源**，
[dou] 运行时依赖 devUrl http://localhost:1420。
[dou] 若要得到可分发的 exe，请改用：
[dou]     cd dou-app && npm run tauri build -- --debug --no-bundle
[dou] 裸 cargo build 产出的 exe 在没有 Vite 的机器上会整窗纯白。
[dou] ======================================================";
        eprintln!("{}", DEV_WARNING);
        println!("cargo:warning=[dou] dev 构建：产物不含前端资源，正式出包请用 npm run tauri build");
    } else if !has_assets {
        panic!(
            "\n\
             ============================================================\n\
             [dou] 致命错误：production 构建缺少前端资源（frontendDist）。\n\
             \n\
             tauri.conf.json 中 frontendDist = \"../dist\"，但该目录下没有可用的\n\
             index.html / assets。若继续编译，将得到一个**能启动但整窗纯白**的\n\
             exe（运行时只会去连 devUrl http://localhost:1420，而那里没有服务）。\n\
             \n\
             正确构建方式（二选一）：\n\
             \n\
               1) 前端 + 应用一体化构建（推荐）：\n\
                    cd <repo>/dou-app\n\
                    npm run tauri build -- --debug --no-bundle\n\
             \n\
               2) 先构建前端，再用 Tauri CLI 编译（不要用 cargo build）：\n\
                    cd <repo>/dou-app\n\
                    npm run build              # 生成 dist/\n\
                    npx tauri build --debug --no-bundle\n\
             \n\
             绝对不要在 src-tauri/ 目录直接执行 `cargo build`。\n\
             ============================================================\n"
        );
    } else {
        println!(
            "cargo:warning=[dou] production 构建已确认前端资源存在：{}",
            dist_dir.display()
        );
    }
}
