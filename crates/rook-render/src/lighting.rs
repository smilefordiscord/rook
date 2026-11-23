//! Lighting on the GPU: local lights in a voxel volume, the sun's shadow
//! map, the sky's cube and BRDF table, and the clouds.

pub(crate) mod clouds;
pub(crate) mod local;
pub(crate) mod shadow;
pub(crate) mod sky;

use crate::*;
