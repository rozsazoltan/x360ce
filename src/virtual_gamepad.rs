use anyhow::{Context, Result};
use vigem_rust::{target::Xbox360, Client, TargetHandle, X360Report};

pub struct VirtualGamepad {
    // Client must outlive target. Target stores a weak reference to it.
    _client: Client,
    target: TargetHandle<Xbox360>,
}

impl VirtualGamepad {
    pub fn connect() -> Result<Self> {
        let client = Client::connect().context("failed to connect to ViGEmBus")?;
        let target = client
            .new_x360_target()
            .plugin()
            .context("failed to plug virtual Xbox 360 controller")?;
        target
            .wait_for_ready()
            .context("virtual Xbox 360 controller did not become ready")?;
        Ok(Self {
            _client: client,
            target,
        })
    }

    pub fn update(&self, report: &X360Report) -> Result<()> {
        self.target
            .update(report)
            .context("failed to update virtual Xbox 360 controller")
    }
}
