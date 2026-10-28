# SYS-08 — Subscriber Core, Service Sessions, Address Management and Policy

**Mission:** be the authoritative logical core for **subscriber identity, authorization, service sessions, IP allocation and QoS/policy** in a Starlink-like broadband simulation. This does **not** assert the real Starlink system runs a 3GPP 5G core.

## Functional decomposition

1. **Subscriber registry:** logical subscriber identity, product/service profile, eligibility, authorized terminal set and tenancy; no real personal data required for experiments.
2. **Authentication/authorization:** simulated identity challenge and policy check; cryptographic detail modeled at an abstract cost/latency level unless explicitly integrating a real stack.
3. **Session manager:** independent logical session `UNREGISTERED → AUTHORIZING → ACTIVE → SUSPENDED → RECOVERING → RELEASED`, revisioned bearer/terminal association and mobility anchoring.
4. **IP address manager:** subscriber IPv6 prefix or addresses, optional private IPv4 lease/pool, lease expiry/reuse, duplicate prevention and routing anchor.
5. **Policy/QoS:** service tiers, allowed regions, bandwidth/priority/rate limits, quota and congestion policies; coordinate radio scheduling with SYS-05.
6. **Accounting:** session usage, traffic volume and uptime records; deterministic fake billing calculation for studies, **not** real payment processing.
7. **Service continuity:** radio bearer changes vs logical session state, anchor updates during ground gateway switches, reauthorization rules and recovery timers.
8. **Optional 3GPP NTN adapter:** a separate configuration profile mapping simulated subscribers to real core functions with explicit standards and identifiers.

## Broadband logical core design

Terminal → access attachment → **service authorization** → session creation → IP assignment and policy distribution → ground subscriber service/traffic anchor → terrestrial egress. The user's active session persists across beam and possibly gateway changes when configured. Do not equate `RadioBearerId` with `ServiceSessionId` or `TerminalId` with `SubscriberId`.

IP policies: IPv6-first model with per-subscriber prefix or lease and a separate optional IPv4/CGNAT path modeled by SYS-09. Address allocation is a deterministic stateful pool; collisions and exhausted pools fail explicitly. For a changing PoP anchor, define either retained session anchor (possibly longer latency) or modeled anchor relocation with control delay, forwarding and NAT-state consequences.

Session control owns authorization decisions from subscription + security context but never bypasses SYS-11 trust checks or SYS-12 regional rules. A disabled subscriber must prevent user-plane egress even when RF attachment is healthy. Rate limiting/QoS policy is enforced where packet scheduling and edge forwarding actually occur; the core distributes policy, it does not secretly create beam capacity.

## Data, interfaces and events

Entities: `SubscriberId`, `SubscriptionProfile`, `TerminalBinding`, `AuthTransaction`, `ServiceSessionId`, `IPLeaseId`, `QoSPolicyId`, `UsageRecord`. Commands: `AuthorizeSubscriber`, `CreateSession`, `BindBearer`, `AssignAddress`, `UpdateSessionAnchor`, `ChangePolicy`, `ReleaseSession`. Events: `AuthorizationApproved/Denied`, `SessionEstablished`, `IPAssigned`, `SessionSuspended/Restored`, `PolicyUpdated`, `UsageRecorded`.

Consumes terminal identity, access radio attach, security verdict, regulatory eligibility, control-plane gateway and PoP choices. Publishes session/address/policy snapshots to SYS-05/07/09/13. Idempotent command IDs prevent duplicate IP lease/session creation after retries. Authentication sequence, simulated latency and denial reasons are visible in trace.

## Technology and external dependencies

**Default P1/P2:** Rust domain state machine, deterministic test identities and address allocator with documented simulated authentication. Generic DHCP/IPAM or external AAA may be integrated later as a separate component. **NR-NTN option:** [Open5GS](https://github.com/open5gs/open5gs) for 5GC/EPC; currently AGPL-3.0 for open-source files and requires legal/deployment review. Pair with a compatible NTN RAN profile (e.g. OAI) rather than treating a 5G core as an unexplained replacement for satellite broadband services.

## Failure modes, acceptance and milestones

Invalid subscription, failed identity check, expired entitlement, duplicate terminal binding, IP pool exhaustion, service-anchor outage, session expiry, subscriber revocation and incompatible QoS request.

**Tests:** user terminal attaches but unauthorized subscriber has no egress; authorized session receives nonduplicated IP; radio handover preserves session/address if configured; gateway change with anchor retained/relocated has explicit interruption; quota/policy affects actual packet scheduling/edge forwarding; test subscribers are synthetic and audit records are complete.

**Build order:** subscriber/session state + synthetic auth → deterministic IP allocation → policy enforcement integration → mobility/anchor scenarios → optional standalone 3GPP core profile.