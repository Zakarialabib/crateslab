//! Sample of a workspace's typed command enum — stands in for
//! `tv-rs-core::CommandId`. `lab-generate` parses this shape.

/// Commands the TV facade understands. The capability set of the target
/// device decides which of these are refused.
#[derive(Debug, Clone)]
pub enum CommandId {
    /// Wake-on-LAN magic packet, no control protocol needed.
    Wake,
    /// Turn the panel off; WoL can bring it back.
    PowerOff,
    /// Read the observed power state with its confidence tag.
    PowerState,
    /// Press a remote key on a paired device.
    KeyPress {
        /// Key name, vendor-specific spelling accepted (e.g. `home`).
        key: String,
        /// Repeat count; None means a single press.
        repeat: Option<u16>,
    },
    /// Launch an app by vendor id.
    Launch {
        /// Vendor app id, e.g. netflix.
        app_id: String,
    },
    /// Set the volume on channels that expose it.
    SetVolume {
        level: u8,
    },
}
