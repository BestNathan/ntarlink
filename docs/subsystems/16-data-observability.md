# SYS-16 — Data Platform, Observability, Experiment Artifacts and Visualization

**Mission:** provide an inspectable, reproducible record of physics/network behavior and a human interface for experiments. Visualization is a **projection** of authoritative subsystem state, never the simulation engine's truth source.

## Functional components

1. **Schema and artifact registry:** versioned scenario, orbital input, domain event and packet-trace schemas with provenance and compatibility checks.
2. **Event/trace ingest:** append-only per-run structured event log, packet and causation correlation, event index by time/actor/flow.
3. **Metrics/aggregation:** per-link capacity, RSS/SINR/BLER, queue, handover, RTT/loss/throughput and service availability rollups.
4. **Experiment artifact store:** run manifest, inputs and hashes, snapshot/checkpoint metadata, raw logs, PCAP, optional Parquet/Arrow and reports.
5. **Query/stream API:** read-only snapshots and time-range queries; WebSocket/SSE timeline at bounded sampling and backpressure.
6. **3D constellation frontend:** CesiumJS Earth/WGS84 positions, orbit traces, ground sites, spot beam footprints, directional ISL/feeder/user links and status.
7. **Experiment control UI:** scenario editor/validator, run launch/pause/replay, permitted fault injection and playback speed; user intent goes through domain-aware APIs.
8. **Comparative analysis:** baseline vs run differences, failure timelines, metrics by shell/region/ground site/flow and raw-data download.

## Data design and scaling

Every artifact is keyed by `run_id` and version and has a content hash and fidelity/model provenance. Domain event envelopes follow [canonical contracts](../design/contracts.md); packet traces additionally capture each hop's link ID, queue delay, propagation, route revision, loss reason and direction. Telemetry carries source simulated time and wall ingest time separately.

Start with deterministic JSONL events and JSON scenario manifests. For large runs use columnar Parquet/Arrow partitions by run/time/event domain with schema evolution rules. Index common queries (`run_id`, `sim_time_ns`, `trace_id`, `satellite_id`) without putting packet IDs into Prometheus labels. Use OpenTelemetry tracing conventions for causal relationships and Prometheus for aggregate metrics. Sampling drops must be disclosed in the artifact manifest; never report an incomplete trace as an exhaustive packet accounting proof.

For the browser, **do not stream millions of raw packet events to React**. Serve viewport/time-bucket summaries, downsample constellation traces, progressively load visible links and packet journeys, use stable entity IDs and GPU-friendly Cesium primitives. Translate frame-tagged ephemeris positions to WGS84 Earth-fixed coordinates at a declared epoch; do not draw TEME xyz as terrestrial positions.

## UI behavior and commands

Principal screens: 3D globe; satellite/contact inspector; beam/terminal map; network graph; packet path and per-hop delay trace; run/event timeline; resource/health panel; experiment comparison and artifact export. Controls are read-only except explicit scenario/run actions. Web control requests go through secure run/orchestration API; UI does not directly mutate satellite, route or core session state.

## Entities and interfaces

Own `ArtifactManifest`, `StoredTrace`, `MetricDefinition`, `TimelineProjection`, `ExperimentReport`, `DashboardState`. APIs: `CreateRunVisualization`, `ListArtifacts`, `GetSnapshotAtTime`, `QueryPacketJourney`, `StreamRunDeltas`, `CompareRuns`, `ExportRawData`. Events: `ArtifactPersisted`, `MetricRollupReady`, `ReportPublished`, `DataGapDetected`.

Consume all subsystem events and SYS-14 runner lifecycle; operator actions depend on SYS-10/11. Observability system may request replay but cannot mint fictitious physical state changes.

## Dependencies and deployment

React/TypeScript and [CesiumJS](https://cesium.com/platform/cesiumjs/) (Apache-2.0) for the UI; Prometheus, OpenTelemetry and Grafana candidates for metrics/trace collection. Python optional for post-run plots and numerical analysis. External map tiles and Cesium ion services can carry different asset terms; prefer reproducible offline/sample assets where tests require deterministic rendering. Avoid forcing a database or high-throughput streaming system before profiling.

## Failure conditions and verification

Event out of order, missing source data, incompatible schema, giant trace download, stale live view, wrong frame conversion, browser GPU limitation, canceled run, aggregate/raw count mismatch and report generation failure. Mark partial data and provide error reason.

**Tests:** packet trace drill-down reconstructs hop path, time and drop reason; globe positions match frame-converted reference; snapshot at time t is consistent with events ≤t; same manifest exports same hash; large-run view remains bounded by time/viewport; missing metrics show “unknown” not fabricated zero. KPIs: ingestion lag, lost event count, query latency, browser update time, artifact integrity and replay parity.

**Build order:** structured trace + artifact manifest → query API → packet journey view → Cesium globe/contact replay → scenario controls and comparisons.