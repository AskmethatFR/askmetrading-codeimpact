# ADR-0001: Core Rust, Hexagon zero-dependency

**Status:** Accepted  
**Date:** 2026-07-08  
**Relations:**  
  related-to: ["architecture-overview", "ADR-0017"]  

## Context

Le projet doit être cross-langage (analyse code .NET, Node.js, Java) avec un core portable.

## Decision

Core en Rust. L'hexagon (domain + application + ports) a **zéro dépendance externe** — pas de tokio, pas de serde. Les dépendances sont injectées au niveau des adapters (secondaries).

## Consequences

- L'hexagon est testable sans `cargo test --features`
- Le core peut être compilé en `wasm32` pour une version web
- Les adapters cross-langage passent par FFI (`extern "C"`)