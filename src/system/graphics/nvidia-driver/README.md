# NVIDIA production driver source closure

MattOS pairs the open GPU kernel modules from the exact `595.84` release with
the unmodified 64-bit userspace and GSP firmware from NVIDIA's matching
`595.84` no-compat32 runfile. The runfile is fetched into an output-owned cache,
verified against `manifest.toml`, and extracted without running NVIDIA's
installer. MattOS never patches or strips proprietary payload bytes.

NVIDIA's driver license permits distribution for use with an OSI-approved
open-source kernel when the binary files are unmodified and the agreement is
provided to recipients. The package builder therefore retains `LICENSE`,
records the original runfile checksum, and may include these packages in the
live ISO. This is an explicit binary source-closure exception, not editable
MattOS source.

Only Turing and newer GPUs are supported by this open-module stack. Pascal
(including GeForce GTX 10-series) requires a separate legacy proprietary-kernel
module milestone. Nouveau and Mesa/NVK remain installed as the fallback.
The NVIDIA module package generates a modprobe selection gate from the matching
runfile's `kernelopen` GPU list. On those devices it permits the official stack
and declines Nouveau; on older devices it declines NVIDIA and permits Nouveau.
Both module stacks remain installed, and no static blacklist removes either
fallback.
