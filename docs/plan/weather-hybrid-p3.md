# P3: teleportation consistency — source implementation

## Physical frame contract
A published cloud frame is identified by world instance, seed, clock revision, weather atlas revision and its two UTC endpoints. The cloud worker still owns immutable source fields; the globe, FPP, rain, flashes and cloud shadows sample the same published physical interval. Rendering quality and camera are not part of the physics identity.

## Invalidation
- When the server acknowledges `MapTeleportResult`, invalidate the **view-dependent** local cloud cut only (serial, queued result, LOD history). Keep existing global/physical state alive while rebuilding.
- Never display old FPP instances if the published camera origin is more than 20 physical km from the current view. The curved globe fallback remains visible.
- Reject asynchronously finished local cuts whose eye is too far from the current observer. This protects against out-of-order worker completion on teleport.
- The request's identity now includes `WorldInstanceId` to distinguish two Earth worlds with coincident seed/clock values.
- Expose published frame identity and view readiness through existing `weather clouds status`.

## Limitations
- Instant visual bit parity is not promised: distant clouds remain optical moments, and regional atmospheric refinement is asynchronous.
- Stale physical fields are retained for rain/surface weather while a view cut rebuilds; UTC lag remains visible in diagnostics.
- This patch adds no temporal reprojection or automatic prefetch on globe click.
- Source changes only; build/static/runtime and teleport/visual checks NOT RUN under visual-first workflow.
