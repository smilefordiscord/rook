//! Scripts (docs/behavior/scripting/): starting and stopping them,
//! destroyed instances freed once no reader holds them, and the frame's
//! parallel phases.

mod collect;
mod parallel_luau;
mod scripts;
