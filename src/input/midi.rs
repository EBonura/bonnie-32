//! MIDI keyboard input for the tracker
//!
//! Native: Uses midir crate for cross-platform MIDI input
//! WASM: Uses Web MIDI API via JavaScript FFI

/// MIDI message types we care about
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiMessage {
    /// Note on: (note_number 0-127, velocity 1-127)
    NoteOn(u8, u8),
    /// Note off: (note_number 0-127)
    NoteOff(u8),
    /// Control change: (controller 0-127, value 0-127)
    ControlChange(u8, u8),
}

// ============================================================================
// WASM Implementation (Web MIDI API)
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::*;

    // FFI bindings to JavaScript functions in index.html
    extern "C" {
        fn bonnie_midi_init();
        fn bonnie_midi_is_connected() -> i32;
        fn bonnie_midi_get_message_count() -> i32;
        fn bonnie_midi_copy_messages(dest_ptr: *mut u8, max_count: usize) -> usize;
        fn bonnie_midi_copy_device_name(dest_ptr: *mut u8, max_len: usize) -> usize;
        fn bonnie_midi_get_device_count() -> i32;
        fn bonnie_midi_connect_device(index: usize) -> i32;
    }

    // Static buffers for FFI
    static mut MESSAGE_BUFFER: [u8; 768] = [0u8; 768]; // 256 messages * 3 bytes
    static mut NAME_BUFFER: [u8; 256] = [0u8; 256];

    pub struct MidiInput {
        initialized: bool,
        held_notes: [bool; 128],
    }

    impl MidiInput {
        pub fn new() -> Self {
            Self {
                initialized: false,
                held_notes: [false; 128],
            }
        }

        pub fn poll(&mut self) -> impl Iterator<Item = MidiMessage> {
            if !self.initialized {
                unsafe { bonnie_midi_init(); }
                self.initialized = true;
            }

            let count = unsafe {
                bonnie_midi_copy_messages(MESSAGE_BUFFER.as_mut_ptr(), 256)
            };

            let mut messages = Vec::with_capacity(count);

            for i in 0..count {
                let offset = i * 3;
                let msg_type = unsafe { MESSAGE_BUFFER[offset] };
                let note = unsafe { MESSAGE_BUFFER[offset + 1] };
                let velocity = unsafe { MESSAGE_BUFFER[offset + 2] };

                let msg = match msg_type {
                    0 => {
                        self.held_notes[note as usize] = false;
                        MidiMessage::NoteOff(note)
                    }
                    1 => {
                        self.held_notes[note as usize] = true;
                        MidiMessage::NoteOn(note, velocity)
                    }
                    2 => MidiMessage::ControlChange(note, velocity),
                    _ => continue,
                };
                messages.push(msg);
            }

            messages.into_iter()
        }

        pub fn is_connected(&self) -> bool {
            unsafe { bonnie_midi_is_connected() != 0 }
        }

        pub fn device_name(&self) -> String {
            if !self.is_connected() {
                return String::new();
            }
            unsafe {
                let len = bonnie_midi_copy_device_name(NAME_BUFFER.as_mut_ptr(), NAME_BUFFER.len());
                if len == 0 {
                    return String::new();
                }
                String::from_utf8_lossy(&NAME_BUFFER[..len]).to_string()
            }
        }

        pub fn list_devices(&self) -> Vec<String> {
            let count = unsafe { bonnie_midi_get_device_count() } as usize;
            (0..count).map(|i| format!("MIDI Device {}", i)).collect()
        }

        pub fn connect_device(&mut self, index: usize) -> Result<(), String> {
            let result = unsafe { bonnie_midi_connect_device(index) };
            if result != 0 {
                Ok(())
            } else {
                Err("Failed to connect to MIDI device".to_string())
            }
        }

        pub fn disconnect(&mut self) {
            // Web MIDI doesn't have explicit disconnect
        }

        /// Check if a MIDI note is currently held down
        pub fn is_note_held(&self, note: u8) -> bool {
            self.held_notes.get(note as usize).copied().unwrap_or(false)
        }
    }

    impl Default for MidiInput {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ============================================================================
// Native Implementation (midir)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::*;
    use midir::{MidiInput as MidirInput, MidiInputConnection};
    use std::sync::{Arc, Mutex};

    /// How often to check for new MIDI devices (in frames)
    const RECONNECT_INTERVAL: u32 = 2;

    pub struct MidiInput {
        /// Shared message queue (filled by callback)
        messages: Arc<Mutex<Vec<MidiMessage>>>,
        /// Active connection (kept alive)
        connection: Option<MidiInputConnection<()>>,
        /// Device name
        device_name: String,
        /// Track held notes
        held_notes: [bool; 128],
        /// Frame counter for periodic reconnection attempts
        reconnect_counter: u32,
    }

    impl MidiInput {
        pub fn new() -> Self {
            let messages = Arc::new(Mutex::new(Vec::new()));
            let mut this = Self {
                messages,
                connection: None,
                device_name: String::new(),
                held_notes: [false; 128],
                reconnect_counter: 0,
            };

            // Auto-connect to first available device
            if this.connect_device(0).is_ok() {
                println!("MIDI: Auto-connected to {}", this.device_name);
            }

            this
        }

        pub fn poll(&mut self) -> impl Iterator<Item = MidiMessage> {
            self.reconnect_counter += 1;
            if self.reconnect_counter >= RECONNECT_INTERVAL {
                self.reconnect_counter = 0;

                // Check if currently connected device is still available
                if self.connection.is_some() {
                    let devices = self.list_devices();
                    if !devices.iter().any(|d| d == &self.device_name) {
                        // Device disconnected
                        println!("MIDI: {} disconnected", self.device_name);
                        self.connection = None;
                        self.device_name.clear();
                        self.held_notes = [false; 128];
                    }
                }

                // Try to connect if no device
                if self.connection.is_none() {
                    if self.connect_device(0).is_ok() {
                        println!("MIDI: Connected to {}", self.device_name);
                    }
                }
            }

            let mut lock = self.messages.lock().unwrap();
            let messages: Vec<_> = lock.drain(..).collect();
            drop(lock);

            // Update held_notes state
            for msg in &messages {
                match msg {
                    MidiMessage::NoteOn(note, _) => self.held_notes[*note as usize] = true,
                    MidiMessage::NoteOff(note) => self.held_notes[*note as usize] = false,
                    _ => {}
                }
            }

            messages.into_iter()
        }

        pub fn is_connected(&self) -> bool {
            self.connection.is_some()
        }

        pub fn device_name(&self) -> String {
            self.device_name.clone()
        }

        pub fn list_devices(&self) -> Vec<String> {
            let Ok(midi_in) = MidirInput::new("bonnie-list") else {
                return Vec::new();
            };

            midi_in.ports()
                .iter()
                .filter_map(|p| midi_in.port_name(p).ok())
                .collect()
        }

        pub fn connect_device(&mut self, index: usize) -> Result<(), String> {
            // Drop existing connection
            self.connection = None;
            self.device_name.clear();

            let midi_in = MidirInput::new("bonnie-tracker")
                .map_err(|e| format!("MIDI init failed: {}", e))?;

            let ports = midi_in.ports();
            let port = ports.get(index)
                .ok_or_else(|| "No MIDI device at index".to_string())?;

            let port_name = midi_in.port_name(port)
                .unwrap_or_else(|_| format!("MIDI Device {}", index));

            let messages = Arc::clone(&self.messages);

            let connection = midi_in.connect(
                port,
                "bonnie-input",
                move |_timestamp, data, _| {
                    if let Some(msg) = parse_midi_message(data) {
                        if let Ok(mut lock) = messages.lock() {
                            lock.push(msg);
                        }
                    }
                },
                (),
            ).map_err(|e| format!("MIDI connect failed: {}", e))?;

            self.device_name = port_name;
            self.connection = Some(connection);

            Ok(())
        }

        pub fn disconnect(&mut self) {
            self.connection = None;
            self.device_name.clear();
        }

        /// Check if a MIDI note is currently held down
        pub fn is_note_held(&self, note: u8) -> bool {
            self.held_notes.get(note as usize).copied().unwrap_or(false)
        }
    }

    impl Default for MidiInput {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Parse raw MIDI bytes into our message type
    fn parse_midi_message(data: &[u8]) -> Option<MidiMessage> {
        if data.is_empty() {
            return None;
        }

        let status = data[0];
        let msg_type = status & 0xF0;

        match msg_type {
            0x90 if data.len() >= 3 => {
                let note = data[1] & 0x7F;
                let velocity = data[2] & 0x7F;
                if velocity > 0 {
                    Some(MidiMessage::NoteOn(note, velocity))
                } else {
                    // Note on with velocity 0 = note off
                    Some(MidiMessage::NoteOff(note))
                }
            }
            0x80 if data.len() >= 3 => {
                let note = data[1] & 0x7F;
                Some(MidiMessage::NoteOff(note))
            }
            0xB0 if data.len() >= 3 => {
                let controller = data[1] & 0x7F;
                let value = data[2] & 0x7F;
                Some(MidiMessage::ControlChange(controller, value))
            }
            _ => None,
        }
    }
}

// Re-export the platform-specific implementation
pub use platform::MidiInput;
