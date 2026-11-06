//! Tes persiapan installer yang butuh internet. Jalankan dengan:
//! cargo test -p cutkit-core --test setup_real -- --ignored --nocapture

use cutkit_core::setup;

#[test]
#[ignore]
fn installs_portable_node_lts() {
    let app = std::env::temp_dir().join(format!("cutkit-node-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app);
    let ver = setup::install_node(&app, |_, _| {}).expect("Node.js terpasang");
    let node = setup::bundled_node(&app);
    let out = std::process::Command::new(&node).arg("--version").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("v{ver}"));
    assert!(!app.join("downloads").exists(), "zip dihapus setelah diekstrak");
    assert_eq!(cutkit_core::mcpcfg::find_node(Some(&app)).as_deref(), Some(node.to_string_lossy().as_ref()), "Node portabel didahulukan");
    let _ = std::fs::remove_dir_all(&app);
}
