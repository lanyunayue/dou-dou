//! `dou-ssh-server`：独立回环 SSH 服务入口（默认 127.0.0.1:2222）。
//!
//! 用法：dou-ssh-server [--bind 127.0.0.1:2222] [--key <host_key>] [--root <remote_root>] [--bash <git_bash>]
//! 也可通过环境变量 GIT_BASH 指定 Git Bash；bash 缺省自动探测。

use dou_ssh_loopback::{find_git_bash, start_server};
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut bind = "127.0.0.1:2222".to_string();
    let mut key = PathBuf::from("loopback_host_key");
    let mut root = PathBuf::from("loopback_remote_root");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bind" => {
                if let Some(v) = args.get(i + 1) {
                    bind = v.clone();
                }
                i += 2;
            }
            "--key" => {
                if let Some(v) = args.get(i + 1) {
                    key = PathBuf::from(v);
                }
                i += 2;
            }
            "--root" => {
                if let Some(v) = args.get(i + 1) {
                    root = PathBuf::from(v);
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    let bash = std::env::var("GIT_BASH").unwrap_or_else(|_| find_git_bash());
    let _ = std::fs::create_dir_all(&root);
    println!("[dou-ssh-server] bind={bind} root={root:?} key={key:?} bash={bash}");

    let handle = start_server(&bind, &key, &root, &bash).expect("启动回环 SSH 服务失败");
    println!("[dou-ssh-server] 已监听 {bind}，Ctrl-C 停止。");
    let _ = tokio::signal::ctrl_c().await;
    handle.shutdown("bye".into());
    println!("[dou-ssh-server] 已停止。");
}
