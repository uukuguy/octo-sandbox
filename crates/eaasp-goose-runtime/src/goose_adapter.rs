// Outcome B: adapter owns a subprocess Child per session + ACP client handle.
// T1 ships the type; T2 wires the subprocess spawn + ACP connect.
pub struct GooseAdapter;

impl GooseAdapter {
    pub fn new() -> Self {
        Self
    }
}
