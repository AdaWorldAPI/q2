// TODO: replace when crate is transcoded
//! R kernel protocol (ZMQ) stub.

/// Represents a connection to an R kernel via the Jupyter/ZMQ protocol.
pub struct RKernel {
    /// Connection endpoint (e.g. "tcp://127.0.0.1:5555")
    endpoint: String,
    connected: bool,
}

#[derive(Debug, Clone)]
pub struct KernelMessage {
    pub msg_type: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct KernelReply {
    pub status: KernelStatus,
    pub output: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelStatus {
    Ok,
    Error,
    Busy,
    Idle,
}

impl RKernel {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            connected: false,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn connect(&mut self) -> Result<(), String> {
        // TODO: implement real ZMQ connection
        self.connected = true;
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
    }

    pub fn execute(&self, code: &str) -> Result<KernelReply, String> {
        if !self.connected {
            return Err("Not connected to kernel".to_string());
        }
        // TODO: implement real ZMQ message exchange
        Ok(KernelReply {
            status: KernelStatus::Ok,
            output: format!("Stub R execution: {}", code),
        })
    }

    pub fn send(&self, message: KernelMessage) -> Result<KernelReply, String> {
        if !self.connected {
            return Err("Not connected to kernel".to_string());
        }
        // TODO: implement real ZMQ message exchange
        Ok(KernelReply {
            status: KernelStatus::Ok,
            output: format!(
                "Stub reply to {}: {}",
                message.msg_type, message.content
            ),
        })
    }
}
