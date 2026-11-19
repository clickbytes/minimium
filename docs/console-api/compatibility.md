# Legacy compatibility matrix

The [machine-readable matrix](../plan/epic-051-inventory.json) enumerates declarations at `fb626d32944b373dd995a5c703ee98eef0db9b35`. Each record identifies its declaring source, existing syntax and aliases, target schema, state owner, scope, consumer, access, read source, commit boundary and migration issue. A result type describes the target contract. Rows with `status: proposed` point to existing data producers and have no legacy invocation.

| Rows | Baseline surface | Count |
| --- | --- | ---: |
| P001–P028 | Client PropertySpec, including the second spelling for meteor audio | 28 |
| P029–P052 | Server PropertySpec, including five shared clock keys | 24 |
| C001–C048 | Client CommandSpec | 48 |
| C049–C064 | Server CommandSpec | 16 |
| C065–C069 | Registered world module commands | 5 |
| C070–C079 | Developer presentation parser branches | 10 |
| S001–S004 | Startup resolver keys missing runtime descriptors | 4 |
| N001–N007 | Designed camera, water, save, explanation and clock reads | 7 |

## Catalog integration

Batch 02 indexes registered commands, properties, the ten concrete presentation forms and installed module commands. Help and list consume this metadata. The command execution backend is still legacy. The client audio spelling is an alias of the meteor sound field. Administrative help resolves installed modules after startup. Existing input is interpreted by its original parser until the binder migration.

### Registered command aliases

| Legacy declaration | Alias | Migration |
| --- | --- | --- |
| C003 · `diagnostics fps` | `fps show` | [#583](https://github.com/clickbytes/minimium/issues/583) |
| C004 · `network stats` | `net stats` | [#580](https://github.com/clickbytes/minimium/issues/580) |
| C005 · `diagnostics state` | `state` | [#580](https://github.com/clickbytes/minimium/issues/580) |
| C006 · `console clear` | `clear` | [#579](https://github.com/clickbytes/minimium/issues/579) |
| C007 · `console quit (alias: exit)` | `exit` | [#580](https://github.com/clickbytes/minimium/issues/580) |
| C008 · `network host local [--threads 1\|2\|auto]` | `host local` | [#580](https://github.com/clickbytes/minimium/issues/580) |
| C009 · `network connect HOST:PORT` | `connect` | [#580](https://github.com/clickbytes/minimium/issues/580) |
| C010 · `network disconnect` | `disconnect` | [#580](https://github.com/clickbytes/minimium/issues/580) |
| C011 · `network ping` | `ping` | [#580](https://github.com/clickbytes/minimium/issues/580) |
| C013 · `movement move X Z (-1..1 each; tangent direction)` | `move` | [#581](https://github.com/clickbytes/minimium/issues/581) |
| C014 · `movement jump` | `jump` | [#576](https://github.com/clickbytes/minimium/issues/576) |
| C015 · `world portal` | `portal` | [#582](https://github.com/clickbytes/minimium/issues/582) |
| C016 · `world teleport: click land on the Earth overview` | `map_telepo` | [#582](https://github.com/clickbytes/minimium/issues/582) |
| C017 · `combat fire` | `fire` | [#581](https://github.com/clickbytes/minimium/issues/581) |
| C018 · `social say TEXT (alias: chat)` | `chat` | [#581](https://github.com/clickbytes/minimium/issues/581) |
| C019 · `social name NAME` | `name` | [#581](https://github.com/clickbytes/minimium/issues/581) |
| C021 · `movement fly [on\|off] - noclip; Space up, Ctrl down; inherits walk speed and accel` | `fly` | [#575](https://github.com/clickbytes/minimium/issues/575) |
| C036 · `camera fov [30..110] (vertical field of view in degrees; default 70)` | `fov` | [#574](https://github.com/clickbytes/minimium/issues/574) |
| C037 · `sky names true\|false` | `show_celestial_names` | [#583](https://github.com/clickbytes/minimium/issues/583) |
| C041 · `time scale DECIMAL (alias timescale; owned local server; 0.1-1000000x or 0)` | `timescale` | [#573](https://github.com/clickbytes/minimium/issues/573) |

`client.audio.meteors` resolves to `client.sky.meteors.sound`. P005 and P006 track both baseline declarations. Alias lookup and canonical access share one state owner. The five shared clock fields each have a client and a server declaration. A host selects an explicit source binding. `world.seed` preserves primary-world generation and startup provenance. The short forms `say` and `quit` require new aliases. [Findings C01–C23](discrepancies.md)

Batch #584 completes P001–P007, C024–C025, C032–C035 and C071–C077. P006 remains a historical matrix row for the alias, with one canonical P005 property and persistent policy. Config accepts either spelling, normalizes equal values, and rejects conflicting values. Local meteor status/spawn/clear and retained bolid previews bind to typed owner operations with bounded reports and exact integer IDs. Debug mode writes may attach a snapshot; reads have no diagnostic side effect. Server bolid interval and meteorid commands retain their separate authoritative owner.

Additional E051 descriptors outside the frozen 142 rows include shower status (A001), script run/status/cancel (A002–A004) and the later script diagnostics. Reconcile them separately at closeout. [Bolid operations](https://github.com/clickbytes/minimium/issues/587) · [Script operations](https://github.com/clickbytes/minimium/issues/593) · [Diagnostics](https://github.com/clickbytes/minimium/issues/595)

The client now registers `say` and `quit`, routes `dev inventory tools` to gameplay and passes quoted FOV values through the property validator. A canonical call, a property verb or a registered word command resolves through the host catalog. Client and administrative hosts expose the pure `ui.console.explain(text)` query.

## Result migration after #571

P008 (`ui.locale`) has a typed read and owner-confirmed set/reset. F1, developer stdin and the terminal share its settings executor and formatter. P001–P007, P009–P028, C001–C048 and the ten presentation forms C070–C079 retain the client display adapter (85 baseline declarations). Server P029–P052 and C049–C069 retain their typed runtime reply views (45 baseline declarations). Their scriptable flags remain false pending their family migrations. Startup-only and proposed rows are tracked separately in the inventory. Batch #596 reconciles these ranges against the final catalog.

Submitted reports client queue admission. OwnerAccepted covers reservations and queued work. Durable requires a storage guarantee. A disconnection after submission leaves the outcome unknown until an authoritative report arrives. For a local camera reset, a file-selected 80° is the startup value. A command-line selection of 90° over that file becomes the startup value instead. Reset clears the session override and restores that value with its original configuration source.

Batch #572 adds an opt-in, authenticated administrative v1 envelope. Text-mode CLI retains its legacy protocol and shutdown verification. [Wire contract](admin-v1.md)

## Shared clock operations

Batch #573 migrates the live `world.time` fields, startup configuration and clock methods to typed reads and mutations. F1 and developer stdin use the active session sample, with the terminal using the same executor. An explicit local demo reports its own source. The server returns its clock snapshot for live fields and its resolved startup settings for `world.time.initial_config`. A solar mutation requires an actor and a matching world instance, content revision and membership epoch at execution. The machine-readable inventory continues to identify baseline spellings and their declaring hosts.

## Camera preferences

The FOV property, camera fov and fov aliases share the finite 30–110 degree validator and the same startup, override and renderer-effective report. The look command consumes degrees through the camera adapter. Camera status exposes presentation coordinates and orientation. Config save returns the destination and byte count after replacement, or the failing I/O stage. Terminal hosts can save their preferences and read FOV provenance. Camera mutations and status require a renderer.

The three player.movement fields and their word aliases read the confirmed session sample. Set and reset use a single-field wire v23 patch. The correlated decision returns to the submitting frontend. Reset requests 5 m/s, 30 m/s² or flight disabled.

The movement jump and jump spellings invoke player.movement.jump(). The request uses the gameplay permission. Queueing and server admission have distinct stages. The final receipt records the physical impulse decision or a coalesced result at the resolved tick.

Batch #579 covers P008, C001, C006 and C050. Client help returns bounded operation IDs with alias and signature metadata from the active catalog. Clear is local to the output view. Locale uses SettingsStore. The admin help route retains its typed built-in and module reports. Client history preserves accepted source spellings, and all three client readers enforce 512 UTF-8 bytes before trimming.

Batch #580 migrates client.connection host_local, connect, disconnect, ping, stats, player.state and client.quit together with their registered aliases. Both hosts use the same process totals, optional session sample, prediction and authoritative player-state projection. An owned process remains distinguishable from a joined endpoint.

Player action bindings preserve raw chat tails, validate decoded chat and player names with the wire predicates, and distinguish queue acceptance from a complete frame write. Fire reports the actual camera or terminal compatibility direction. Owner inventory snapshots and untagged shot events retain their independent identities.

Batch #582 routes `world portal`/`portal`, `dev goto`, and `set_altitude` through actor intents. `dev goto` uses a CPU geographic conversion in every host; `world teleport`/`map_telepo` arms a desktop map click and never means immediate teleport. Active source membership, pending destination and owner commit remain separate. Wire v26 adds correlated portal, goto and altitude results; map selection itself stays local.


Batch E051-16 binds P010-P020 and seven local renderer query declarations to typed results. Desktop settings and legacy aliases use SettingsStore plus effective input or renderer state. The text host exposes the same catalog and reports renderer capability errors for presentation queries.

Batch #585 completes P029-P034, P036-P038, P040-P042, P044-P045,
S001-S004 and C051-C053/C055. The four S rows are now registered
restart-required properties and remain baseline inventory rows, not duplicate
owners. Canonical `server.players.list(cursor?, limit=64)` and typed host pages
use the bounded reader; legacy `players list [CURSOR]` retains 256 rows.
Status, metrics and stop use owner payloads. Stop's committed request is
separate from process completion. Configuration reads share `PropertyValue`
provenance and the server resolver's defaults and validation.

Batch #586 binds primary generation properties and world/object/component metadata pages to the existing server owner. Legacy world and entity lists keep a 256-row default; canonical lists default to 64 and cap at 256 with a 32 KiB page budget. Both formats retain the entity/player union and exact IDs. Seed aliases still read the primary manifest and preserve startup provenance. The v3 cursor replaces v2; old tokens expire explicitly. A canonical `world.entities.inspect` request with maximal u128/u64 IDs, maximal correlation, cursor and limit occupies 496 of 512 bytes with `expected_context:null`; these admin queries are actor independent and require no world context. The trusted host query port also exposes a metadata-only component page.
