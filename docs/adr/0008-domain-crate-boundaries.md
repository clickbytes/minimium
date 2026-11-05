# ADR 0008: World and environment domain crates

Status: accepted for the current extraction.

`minimium-core` is limited to foundational identifiers and platform-neutral time/environment values. `minimium-world` owns world manifests, topologies, addressing, geometry, maps and entity/component contracts. `minimium-environment` owns observer/sky and solar-system catalogue contracts and may depend on world definitions. Neither domain depends on application, platform, protocol, renderer or storage packages.

World definitions, topology, geometry and neutral entity/component data belong to `minimium-world`. Mutable simulation state, movement, collision, combat and step results belong to `minimium-simulation`, which depends on world contracts. Client prediction and server authority use the same integrator.
