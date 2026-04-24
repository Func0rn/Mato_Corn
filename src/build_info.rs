pub const BUILD_ID: &str = env!("MATO_BUILD_ID");

pub fn current_build_id() -> String {
    BUILD_ID.to_string()
}
