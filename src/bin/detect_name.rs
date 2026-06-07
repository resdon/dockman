use std::collections::HashMap;
use smithay_client_toolkit::foreign_toplevel_list::{ForeignToplevelList, ForeignToplevelListHandler};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState, RegistryHandler};
use smithay_client_toolkit::reexports::protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1;
use wayland_client::globals::registry_queue_init;
use wayland_client::{Connection, QueueHandle, Proxy}; // Added Proxy here

struct DiagnosticApp {
    registry_state: RegistryState,
    toplevel_list: Option<ForeignToplevelList>,
    window_cache: HashMap<u32, (String, String)>, // Stores (AppID, Title)
}

impl ProvidesRegistryState for DiagnosticApp {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers![DiagnosticApp];
}

impl RegistryHandler<DiagnosticApp> for DiagnosticApp {
    fn new_global(
        _state: &mut Self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
        _version: u32,
    ) {}

    fn remove_global(
        _state: &mut Self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
    ) {}
}

impl ForeignToplevelListHandler for DiagnosticApp {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelList {
        self.toplevel_list.as_mut().unwrap()
    }

    fn new_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: ExtForeignToplevelHandleV1,
    ) {
        let id = toplevel.id().protocol_id();
        println!("[PROXY GENERATED] Window Protocol ID: {} allocated on socket.", id);
        self.window_cache.entry(id).or_insert_with(|| ("".to_string(), "".to_string()));
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: ExtForeignToplevelHandleV1,
    ) {
        let state = self.foreign_toplevel_list_state();
        let id = toplevel.id().protocol_id();

        if let Some(info) = state.info(&toplevel) {
            let cached = self.window_cache.entry(id).or_insert_with(|| ("".to_string(), "".to_string()));

            let current_app_id = if info.app_id.is_empty() { "[BLANK APP_ID]" } else { &info.app_id };
            let current_title = if info.title.is_empty() { "[BLANK TITLE]" } else { &info.title };

            if info.app_id != cached.0 || info.title != cached.1 {
                println!("----------------------------------------");
                println!("[UPDATE RECEIVED] Window ID: {}", id);
                println!("  App ID : {}  (Was: '{}')", current_app_id, cached.0);
                println!("  Title  : {}  (Was: '{}')", current_title, cached.1);
                println!("----------------------------------------");

                self.window_cache.insert(id, (info.app_id.clone(), info.title.clone()));
            }
        }
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: ExtForeignToplevelHandleV1,
    ) {
        let id = toplevel.id().protocol_id();
        if let Some((app_id, title)) = self.window_cache.remove(&id) {
            let display_name = if app_id.is_empty() { "[BLANK APP_ID]" } else { &app_id };
            println!("[WINDOW CLOSED] ID: {} | App ID: {} | Last Title: {}", id, display_name, title);
        } else {
            println!("[WINDOW CLOSED] ID: {} (Was never cached with details)", id);
        }
    }
}

smithay_client_toolkit::delegate_registry!(DiagnosticApp);
smithay_client_toolkit::delegate_foreign_toplevel_list!(DiagnosticApp);

fn main() {
    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland compositor");
    let (globals, mut event_queue) = registry_queue_init::<DiagnosticApp>(&conn)
        .expect("Failed to initialize registry queue");
    let qh = event_queue.handle();

    let mut app = DiagnosticApp {
        registry_state: RegistryState::new(&globals),
        toplevel_list: None,
        window_cache: HashMap::new(),
    };

    let manager = ForeignToplevelList::new(&globals, &qh);
    app.toplevel_list = Some(manager);

    event_queue.roundtrip(&mut app).expect("First roundtrip failed");
    event_queue.roundtrip(&mut app).expect("Second roundtrip failed");

    println!("=== RUNNING DIAGNOSTIC STREAM (Ctrl+C to stop) ===");

    loop {
        event_queue.blocking_dispatch(&mut app).expect("Wayland socket error");
    }
}
