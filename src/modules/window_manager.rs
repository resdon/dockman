use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1;

#[derive(Debug, Default, Clone)]
pub struct TrackedWindow {
    pub title: String,
    pub app_id: String,
    pub is_activated: bool,
    pub is_minimized: bool,
    // Hit-testing boundary tracking metrics
    pub x_start: usize,
    pub x_end: usize,
}

pub fn parse_window_states(state_bytes: &[u8]) -> (bool, bool) {
    let mut activated = false;
    let mut minimized = false;
    for chunk in state_bytes.chunks_exact(4) {
        if let Ok(array_bytes) = chunk.try_into() {
            let value = u32::from_ne_bytes(array_bytes);
            if let Ok(state_enum) = zwlr_foreign_toplevel_handle_v1::State::try_from(value) {
                match state_enum {
                    zwlr_foreign_toplevel_handle_v1::State::Activated => activated = true,
                    zwlr_foreign_toplevel_handle_v1::State::Minimized => minimized = true,
                    _ => {}
                }
            }
        }
    }
    (activated, minimized)
}
