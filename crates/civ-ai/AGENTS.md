# civ-ai — local rules

Not yet implemented. See docs/architecture.md section 3 for this crate's
remit and the layer it sits in, and the milestone list in the plan for when it
lands.

The layering rule applies from the first line of code: this crate may depend
only on crates at a strictly lower layer, and `cargo xtask check-deps` will
fail the build otherwise.
