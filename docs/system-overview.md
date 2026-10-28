# ntarlink system overview

This document defines the mission and catalog. The authoritative **detailed specifications** are in [docs/subsystems](subsystems/README.md).

## Mission

Develop a reproducible Starlink-like LEO broadband internet simulation/emulation platform covering spacecraft/orbits, optical ISL, user access, feeder gateways, subscriber services, terrestrial backbone, network control, operations, security, environment and user experience. Model fidelity is explicitly labeled; do not assume access to proprietary Starlink PHY/MAC.

## 16 logical subsystems

| ID | System | Role | Detailed design |
|---|---|---|---|
| SYS-01 | 轨道动力学与星历 | 轨道与几何真值服务 | [Specification](subsystems/01-orbital-dynamics.md) |
| SYS-02 | 星座与航天器 | 空间资产及载荷能力的权威状态 | [Specification](subsystems/02-spacecraft.md) |
| SYS-03 | 星间通信与星上转发 | 卫星之间的传输和星上数据平面 | [Specification](subsystems/03-space-network.md) |
| SYS-04 | 用户终端与CPE | 用户侧设备、天线和本地网络边界 | [Specification](subsystems/04-terminal.md) |
| SYS-05 | 用户接入网络 | 用户终端至卫星的无线接入控制和传输 | [Specification](subsystems/05-access-network.md) |
| SYS-06 | 地面站与馈电网络 | 卫星馈电链路及地面射频基础设施 | [Specification](subsystems/06-ground-segment.md) |
| SYS-07 | 全网控制与流量工程 | 动态卫星网络拓扑、路径及容量决策 | [Specification](subsystems/07-network-control.md) |
| SYS-08 | 用户核心网与业务控制 | 用户身份、服务会话、地址与策略的逻辑核心 | [Specification](subsystems/08-subscriber-core.md) |
| SYS-09 | 地面骨干网与互联网出口 | 地面传输、PoP和公网互联 | [Specification](subsystems/09-terrestrial-network.md) |
| SYS-10 | 网络运营与任务控制 | 卫星和网络运营操作中心 | [Specification](subsystems/10-operations.md) |
| SYS-11 | 安全与信任 | 跨系统身份、授权和威胁模型 | [Specification](subsystems/11-security.md) |
| SYS-12 | 频谱监管与传播环境 | 约束无线服务可用性的外部环境真值 | [Specification](subsystems/12-environment.md) |
| SYS-13 | 服务质量与用户体验 | 从终端视角量化网络质量与可用性 | [Specification](subsystems/13-service-assurance.md) |
| SYS-14 | 仿真编排与执行内核 | 全系统确定性实验执行基础设施 | [Specification](subsystems/14-simulation.md) |
| SYS-15 | 网络仿真与外部协议适配 | 让真实IP流量和第三方通信模拟器参与实验 | [Specification](subsystems/15-emulation.md) |
| SYS-16 | 数据、可观测与可视化 | 跨系统数据证据、分析和操作入口 | [Specification](subsystems/16-data-observability.md) |

## End-to-end broadband data plane

LAN → terminal/CPE → user RF/beam → satellite payload → optional optical ISLs → feeder link → ground station → PoP/backhaul → internet edge → destination. Reverse direction can have an asymmetric path.

## Control plane

Orbit/spacecraft/environment state → contact feasibility → access and network decisions → installed forwarding state → operations and assurance feedback. Subscriber core owns service session/policy, not beam PHY. A feeder gateway, subscriber service gateway and internet egress are distinct roles.

## Architectural principles

- **Bounded contexts before languages.** Rust, Python and TypeScript can coexist inside each subsystem.
- **One monorepo, many domains.** A logical system is not automatically a microservice; deterministic in-process execution is valid.
- **Contracts before coupling.** Authoritative state ownership, typed events, units, clock domains and trace correlation.
- **Multiple fidelity profiles.** Analytical, discrete-event, protocol-study, IP emulation and optional HIL are not interchangeable.
- **Standards vs proprietary.** 3GPP NTN/Open5GS is an optional profile, not assumed Starlink broadband core architecture.
- **Acceptance over folder count.** See [system scenarios](system-scenarios.md).
