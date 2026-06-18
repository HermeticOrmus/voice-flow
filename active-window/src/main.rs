// ormus-active-window: report the currently focused (activated) toplevel on COSMIC.
//
// COSMIC does not implement wlr-foreign-toplevel-management. We combine two protocols
// via cctk's ToplevelInfoState:
//   - ext-foreign-toplevel-list-v1  -> stable per-window `identifier` (no state)
//   - cosmic-toplevel-info-v1 (v2)  -> per-toplevel `state` incl. Activated
// One-shot: connect, roundtrip until synced, find the Activated toplevel, print, exit.

use std::process::exit;

use cctk::{
    cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
    delegate_toplevel_info,
    sctk::{
        delegate_registry,
        registry::{ProvidesRegistryState, RegistryState},
        registry_handlers,
    },
    toplevel_info::{ToplevelInfo, ToplevelInfoHandler, ToplevelInfoState},
    wayland_client::{globals::registry_queue_init, Connection, QueueHandle},
};
// The ext handle type lives in wayland-protocols, re-exported through cctk.
use cctk::wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1 as ftl_handle;

struct State {
    registry_state: RegistryState,
    toplevel_info: ToplevelInfoState,
    // Set once we have observed at least one info_done sync from the cosmic protocol.
    synced: bool,
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers!();
}

impl ToplevelInfoHandler for State {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info
    }

    fn new_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _toplevel: &ftl_handle::ExtForeignToplevelHandleV1,
    ) {
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _toplevel: &ftl_handle::ExtForeignToplevelHandleV1,
    ) {
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _toplevel: &ftl_handle::ExtForeignToplevelHandleV1,
    ) {
    }

    fn info_done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>) {
        // The cosmic toplevel-info protocol emits Done after a batch of state updates.
        self.synced = true;
    }
}

delegate_toplevel_info!(State);
delegate_registry!(State);

fn is_activated(info: &ToplevelInfo) -> bool {
    info.state
        .contains(&zcosmic_toplevel_handle_v1::State::Activated)
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

fn main() {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("--json") => "json",
        Some("--list") => "list",
        _ => "default",
    };

    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ormus-active-window: cannot connect to Wayland: {e}");
            exit(1);
        }
    };

    let (globals, mut event_queue) = match registry_queue_init::<State>(&conn) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ormus-active-window: registry init failed: {e}");
            exit(1);
        }
    };
    let qh = event_queue.handle();
    let registry_state = RegistryState::new(&globals);

    let toplevel_info = match ToplevelInfoState::try_new(&registry_state, &qh) {
        Some(t) => t,
        None => {
            eprintln!(
                "ormus-active-window: compositor does not expose ext-foreign-toplevel-list-v1 + cosmic-toplevel-info-v1"
            );
            exit(1);
        }
    };

    let mut state = State {
        registry_state,
        toplevel_info,
        synced: false,
    };

    // Pump events until synced or a safety budget elapses (~1s / 50 iterations).
    // We require: at least one info_done AND at least one toplevel reporting Activated,
    // but never block past the budget.
    let mut iterations = 0;
    loop {
        if event_queue.roundtrip(&mut state).is_err() {
            break;
        }
        iterations += 1;

        let have_activated = state.toplevel_info.toplevels().any(is_activated);
        if state.synced && have_activated {
            break;
        }
        if iterations >= 50 {
            break;
        }
        // A short sleep keeps the busy loop from spinning the CPU while the
        // compositor finishes emitting the initial state burst.
        std::thread::sleep(std::time::Duration::from_millis(15));
    }

    let focused = state.toplevel_info.toplevels().find(|t| is_activated(t));

    match mode {
        "list" => {
            for t in state.toplevel_info.toplevels() {
                let marker = if is_activated(t) { '*' } else { ' ' };
                println!(
                    "{} | app_id=\"{}\" title=\"{}\" id=\"{}\"",
                    marker, t.app_id, t.title, t.identifier
                );
            }
            exit(0);
        }
        "json" => match focused {
            Some(t) => {
                println!(
                    "{{\"identifier\":\"{}\",\"app_id\":\"{}\",\"title\":\"{}\"}}",
                    json_escape(&t.identifier),
                    json_escape(&t.app_id),
                    json_escape(&t.title)
                );
                exit(0);
            }
            None => exit(1),
        },
        _ => match focused {
            Some(t) => {
                if !t.identifier.is_empty() {
                    println!("{}", t.identifier);
                } else {
                    println!("{}\t{}", t.app_id, t.title);
                }
                exit(0);
            }
            None => exit(1),
        },
    }
}
