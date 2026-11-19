//! HDR displays. Reruns records SDR video; when the game's display runs in
//! HDR, what the capture hands over is clipped to SDR, so bright highlights
//! blow out. Knowing that (and the display's levels) is the first step to
//! tone mapping it properly. See `docs/HDR.md`.

use serde::Serialize;

/// A display running in HDR (Windows: "Use HDR" on, BT.2020 / PQ output).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HdrDisplay {
    /// What SDR white is shown at, in nits: Windows' "SDR content
    /// brightness" slider (80 nits at its lowest). A tone mapper maps
    /// this level to SDR white. 0 if it couldn't be read (yet).
    pub sdr_white_nits: f32,
    /// The display's reported peak brightness in nits (0 if it doesn't
    /// say).
    pub peak_nits: f32,
}
