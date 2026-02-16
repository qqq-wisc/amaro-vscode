# Amaro Examples

This folder contains example `.qmrl` files demonstrating different quantum routing strategies.

## Files

### `nisq.qmrl`
**NISQ (Noisy Intermediate-Scale Quantum) Routing**

Simple edge-based routing for near-term quantum devices. Routes CX gates only when qubits are already adjacent on the hardware graph. Uses swap operations to move qubits closer together.

**Key Features:**
- Direct edge checking with `Arch.contains_edge()`
- Identity swap transition for no-ops
- Minimal architecture requirements (width, height)

---

### `scmr.qmrl`
**SCMR (Surface Code Mapping and Routing)**

Advanced routing for fault-tolerant quantum computing with magic state distillation. Handles both CX and T gates with path-based realizations.

**Key Features:**
- Vertical/horizontal neighbor constraints
- Magic state qubit integration
- Path-based gate realizations
- Avoids previously implemented gates

---

### `ilq.qmrl`
**ILQ (Interleaved Lattice Qubits)**

Routing for layered quantum architectures with vertical stacking. Qubits are organized in layers, and the router handles inter-layer and intra-layer connections differently.

**Key Features:**
- Layer-aware routing (`stack_size` parameter)
- Conditional logic based on qubit layers
- Range expressions for layer iteration
- Same-layer fast paths

---

### `mqlss.qmrl`
**MQLSS (Magic State Lattice Surgery Scheduling)**

Routing for Pauli measurements using Steiner tree construction. Optimizes for multi-qubit Pauli operators (X, Y, Z) by finding minimal spanning trees.

**Key Features:**
- Steiner tree routing
- Gate index methods (x_indices, y_indices, z_indices)
- Tree-based realizations
- Pauli measurement support

---

## Testing Examples

All examples should load without errors in the VS Code extension:

1. Open the extension development host (`F5` in VS Code)
2. Open any `.qmrl` file from this folder
3. Check the "Problems" panel — should show 0 errors

## Common Patterns

### Architecture Block
All examples define an `ArchInfo` block with hardware parameters:
```amaro
ArchInfo:
    Arch{width : Int, height : Int, magic_state_qubits : Vec<Location>, ...}
```

### Gate Realization
The `realize_gate` field defines how to implement gates on the hardware:
```amaro
realize_gate = if (Gate.gate_type()) == CX then ... else ...
```

### Transitions
The `get_transitions` field defines available routing moves:
```amaro
get_transitions = map(|x| -> Transition{edge = x}, Arch.edges())
```
