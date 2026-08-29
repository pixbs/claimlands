# ADR 0001 — Rust and wgpu, with a thin custom engine

**Status:** accepted · 2026-08-29

## Context

The game ships to iOS and Android, is written once, and should sit as close to
the hardware as reasonable. All models and textures are generated procedurally
at runtime, so there is no asset pipeline to inherit from an engine.

Candidates: Rust + wgpu with a hand-written engine; Rust + Bevy; Godot 4 with
Rust via gdext; Unity.

## Decision

**Rust, `wgpu`, no engine framework.** Thin Swift and Kotlin shells own the
platform surface.

## Why

- **Bevy's mobile support is not ready.** Its own documentation still describes
  shipping to iOS and Android as "possible, not easy" — no Android Studio
  integration, gaps in sensor support. It also ships breaking releases roughly
  quarterly, and that churn is precisely what causes the cross-agent
  regressions this repository is built to prevent.
- **Godot** gives mature mobile export but adds a full engine runtime and a
  scripting boundary, makes procedural mesh generation clumsier, and gives up
  the "close to hardware" goal.
- **Unity** is not close to the hardware and is a poor fit for a codebase meant
  to be driven by automated agents.
- **wgpu compiles to WebGPU**, which means the same renderer runs in a browser.
  That is what makes automated visual regression testing possible in CI without
  a device farm — a decisive advantage for this project specifically.
- The prototype already hand-rolls its renderer with very specific requirements
  (render at one third resolution, nearest filtering, a two-pass frame, a custom
  cloud shader). An engine would be fought, not used.
- Rust gives a deterministic, allocation-predictable simulation with no GC
  pauses, which the replay and future multiplayer features depend on.

## Consequences

- We write our own camera, input, audio and UI. That is perhaps three thousand
  lines we would otherwise get free.
- No engine editor. The level editor is ours to build (M4).
- In exchange: a tiny binary, no upstream churn, and a simulation that runs
  headless at millions of turns per second.
