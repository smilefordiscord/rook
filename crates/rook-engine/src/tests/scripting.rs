//! Scripts (docs/behavior/scripting/): starting and stopping them,
//! destroyed instances freed once no reader holds them, purchases,
//! HttpService's requests, and the frame's parallel phases.

mod collect;
mod http_requests;
mod marketplace;
mod parallel_luau;
mod scripts;
mod teleports;
