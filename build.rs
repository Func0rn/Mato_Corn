use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let build_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string());
    println!("cargo:rustc-env=MATO_BUILD_ID={build_id}");
}
