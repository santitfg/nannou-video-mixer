//! Inputs that take any number of cables.
//!
//! Most ports hold one cable (plugging another replaces it). A *multi*
//! port, like TouchDesigner's multi-input operators, keeps every cable
//! dropped on it: the module sees them as a list, in the order they were
//! plugged. Instead of growing a new inlet per layer, the module has one
//! port and the cables stack up on it.
//!
//! To make another module's port multi, list it in `multi_port`; the
//! module's evaluation then asks `sources` instead of `input_source`.

use super::{Connection, ModuleKind};

/// The input port of `kind` that accepts any number of cables, if any.
pub fn multi_port(kind: ModuleKind) -> Option<usize> {
    match kind {
        ModuleKind::Stack | ModuleKind::Select => Some(0),
        _ => None,
    }
}

pub fn is_multi(kind: ModuleKind, input: usize) -> bool {
    multi_port(kind) == Some(input)
}

/// Every module wired into `to`'s input `input`, in plugging order.
pub fn sources(connections: &[Connection], to: u64, input: usize) -> Vec<u64> {
    source_ports(connections, to, input).into_iter().map(|(from, _)| from).collect()
}

/// Like `sources`, with the output port each cable leaves from.
pub fn source_ports(connections: &[Connection], to: u64, input: usize) -> Vec<(u64, usize)> {
    connections.iter().filter(|c| c.to == to && c.to_input == input).map(|c| (c.from, c.from_output)).collect()
}

/// Adds the cable `from:from_output -> to:input`. A multi port keeps its
/// other cables (but never holds the same source port twice); a normal
/// port is replaced.
pub fn connect(connections: &mut Vec<Connection>, kind: ModuleKind, from: u64, from_output: usize, to: u64, to_input: usize) {
    if is_multi(kind, to_input) {
        connections.retain(|c| !(c.to == to && c.to_input == to_input && c.from == from && c.from_output == from_output));
    } else {
        connections.retain(|c| !(c.to == to && c.to_input == to_input));
    }
    connections.push(Connection { from, from_output, to, to_input });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_port_keeps_every_cable_in_order() {
        let mut cs = Vec::new();
        for from in [3, 1, 2] {
            connect(&mut cs, ModuleKind::Stack, from, 0, 9, 0);
        }
        connect(&mut cs, ModuleKind::Stack, 1, 0, 9, 0); // same source again: moved to the end, not duplicated
        assert_eq!(sources(&cs, 9, 0), [3, 2, 1]);
    }

    #[test]
    fn normal_port_is_replaced() {
        let mut cs = Vec::new();
        connect(&mut cs, ModuleKind::Blend, 1, 0, 9, 0);
        connect(&mut cs, ModuleKind::Blend, 2, 0, 9, 0);
        assert_eq!(sources(&cs, 9, 0), [2]);
    }

    #[test]
    fn different_ports_of_one_module_are_different_cables() {
        let mut cs = Vec::new();
        connect(&mut cs, ModuleKind::Select, 4, 0, 9, 0);
        connect(&mut cs, ModuleKind::Select, 4, 2, 9, 0);
        connect(&mut cs, ModuleKind::Select, 4, 0, 9, 0);
        assert_eq!(source_ports(&cs, 9, 0), [(4, 2), (4, 0)]);
    }
}
