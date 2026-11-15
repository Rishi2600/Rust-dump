//! A tiny user-space simulation of an operating system kernel.

pub mod assembler;
pub mod cpu;
pub mod error;
pub mod frame;
pub mod instr;
pub mod memory;
pub mod mmu;
pub mod paging;
