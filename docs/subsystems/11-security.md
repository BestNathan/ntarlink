# SYS-11 — Identity, Trust, Authorization and Security Experiments

**Mission:** define and enforce security/trust policy across control interfaces, subscriber service and operational actions. Model credible security properties without inventing proprietary Starlink cryptography or shipping insecure pseudo-crypto as if production ready.

## Core components

| Component | Position |
|---|---|
| Trust model | principals, service identities, terminal/subscriber relationships, device certificates/credentials as abstract identities |
| Authorization engine | permission policy for operator, service, subsystem and terminal actions |
| Credential lifecycle | creation, validity, rotation, revocation and expiry of simulated credentials |
| Transport protection | optional real TLS/mTLS for live APIs; simulated crypto overhead on modeled links |
| Management-plane isolation | separate management/control/data endpoints and permitted API audiences |
| Audit ledger | immutable run-linked records of decisions, actions and security-relevant state transitions |
| Threat experiment engine | replay, injection, denial of service, spoofed command, resource exhaustion and jamming **models** |

## Threat boundaries

Assets: spacecraft control commands, routing updates, ground station operations, subscriber sessions, telemetry provenance and experiment artifacts. Trust assumptions differ: in-process simulation can pass a typed authenticated context; external worker APIs require actual authentication and least-privilege credentials. User-plane packets may be treated as encrypted payloads with explicit overhead and protected/unprotected toggles; the simulator should not claim compatibility with actual production SpaceX tunnel formats or keys.

Define principal/subject/action/resource/policy-decision tuples. A request has `principal_id`, `action`, `target_id`, `context`, `expected_revision` and `command_id`; policy yields `ALLOW` or `DENY` with reason. SYS-10 owns approval workflow, SYS-11 owns access decision, destination subsystem owns actual state mutation. The system must reject a route installation or spacecraft mode change without authorization, even if the command's source looks like an internal event.

For deliberate attack scenarios use **bounded modeled behavior**: probabilistic telemetry delay, flood input at declared rates, forged command rejection, replay-idempotency test, link availability under interference from SYS-12. No offensive payloads or live credential harvesting are needed.

## Interfaces and events

Entities: `Principal`, `TrustAnchor`, `Credential`, `AuthorizationPolicy`, `SecurityFinding`, `AuditEntry`. APIs: `AuthorizeCommand`, `RotateCredential`, `RevokePrincipal`, `ValidateManagementConnection`, `RecordAuditEvent`. Events: `AuthorizationAllowed/Denied`, `CredentialExpired`, `TrustPolicyChanged`, `SecurityIncidentRecorded`.

Consumers: SYS-08 subscriber service, SYS-10 operations, SYS-07 controller and SYS-15 worker orchestration. Expose only redacted identifiers in public experiment reports; experiments should use synthetic test identities.

## Dependencies and isolation

Use maintained Rust TLS and cryptographic libraries only when implementing **real** management API transport, never write custom crypto primitives. External OAI/Open5GS security is specific to its optional 3GPP profile; do not graft it onto baseline broadband access without defined contracts. Privileged Linux emulators run with limited capabilities, constrained resource quotas, isolated network namespaces and controlled input files.

## Failures and acceptance

Expired credential, authorization role mismatch, replayed command ID, untrusted telemetry origin, audit write failure, worker compromise simulation and management-plane DoS. Fail **closed** on unauthorized operational commands. Modeling degraded service due to an attack should remain separate from real implementation vulnerabilities.

**Tests:** prohibited operator cannot disable satellite; authorized approval executes once; replay rejected/deduplicated; expired subscriber entitlement terminates policy service; security event trace causally ties to incident, with no secret-bearing payload in artifacts. Measure deny count, time-to-authorization, audit coverage, policy revision and simulated attack impact.