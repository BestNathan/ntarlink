# Detailed subsystem design index

These 16 documents are the **normative subsystem decomposition** for ntarlink. Every document defines position, core entities, detailed internal capabilities, ownership, dependencies, interfaces, events, failure modes, upstream candidates, acceptance and milestones.

- **SYS-01** [轨道动力学与星历](01-orbital-dynamics.md) — 轨道与几何真值服务
- **SYS-02** [星座与航天器](02-spacecraft.md) — 空间资产及载荷能力的权威状态
- **SYS-03** [星间通信与星上转发](03-space-network.md) — 卫星之间的传输和星上数据平面
- **SYS-04** [用户终端与CPE](04-terminal.md) — 用户侧设备、天线和本地网络边界
- **SYS-05** [用户接入网络](05-access-network.md) — 用户终端至卫星的无线接入控制和传输
- **SYS-06** [地面站与馈电网络](06-ground-segment.md) — 卫星馈电链路及地面射频基础设施
- **SYS-07** [全网控制与流量工程](07-network-control.md) — 动态卫星网络拓扑、路径及容量决策
- **SYS-08** [用户核心网与业务控制](08-subscriber-core.md) — 用户身份、服务会话、地址与策略的逻辑核心
- **SYS-09** [地面骨干网与互联网出口](09-terrestrial-network.md) — 地面传输、PoP和公网互联
- **SYS-10** [网络运营与任务控制](10-operations.md) — 卫星和网络运营操作中心
- **SYS-11** [安全与信任](11-security.md) — 跨系统身份、授权和威胁模型
- **SYS-12** [频谱监管与传播环境](12-environment.md) — 约束无线服务可用性的外部环境真值
- **SYS-13** [服务质量与用户体验](13-service-assurance.md) — 从终端视角量化网络质量与可用性
- **SYS-14** [仿真编排与执行内核](14-simulation.md) — 全系统确定性实验执行基础设施
- **SYS-15** [网络仿真与外部协议适配](15-emulation.md) — 让真实IP流量和第三方通信模拟器参与实验
- **SYS-16** [数据、可观测与可视化](16-data-observability.md) — 跨系统数据证据、分析和操作入口

See [overall catalog](../system-overview.md), [boundaries](../subsystem-boundaries.md), [E2E acceptance](../system-scenarios.md). A subsystem is a logical bounded context, **not necessarily a separate service**. Current repository code has not yet migrated to the target directory layout.
