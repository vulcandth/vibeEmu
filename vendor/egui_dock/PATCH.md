# Local patch: egui_dock 0.19.1

Source: the crates.io 0.19.1 package (MIT; LICENSE retained).

The release uses unmaintained `paste` (RUSTSEC-2024-0436) only to concatenate
three identifiers in separator layout macros. Supply those identifiers as
parameters to the existing `duplicate!` macros instead. This removes the
`paste` dependency without changing the expanded layout code or upgrading egui.

Remove this patch when a compatible upstream release removes that dependency.
