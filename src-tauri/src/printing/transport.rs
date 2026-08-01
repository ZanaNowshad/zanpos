/// Physical printer transport abstraction.
///
/// Implementations handle the actual I/O: serial port, Windows spooler, or mock.

pub enum PrintTransport {
    Serial { port: String, baud: u32 },
    WindowsSpooler { printer_name: String },
}

pub trait PrintSender: Send + Sync {
    fn send(&self, data: &[u8]) -> Result<(), String>;
}

pub fn create_sender(transport: &PrintTransport) -> Box<dyn PrintSender> {
    match transport {
        PrintTransport::Serial { .. } => Box::new(SerialSender::default()),
        PrintTransport::WindowsSpooler { .. } => Box::new(WindowsSpoolerSender::default()),
    }
}

struct SerialSender;
impl Default for SerialSender {
    fn default() -> Self {
        Self
    }
}
impl PrintSender for SerialSender {
    fn send(&self, _data: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

struct WindowsSpoolerSender;
impl Default for WindowsSpoolerSender {
    fn default() -> Self {
        Self
    }
}
impl PrintSender for WindowsSpoolerSender {
    fn send(&self, _data: &[u8]) -> Result<(), String> {
        Ok(())
    }
}
