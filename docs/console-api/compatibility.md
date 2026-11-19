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

Additional E051 descriptors outside the frozen 142 rows include shower status (A001), script run/status/cancel (A002–A004) and the later script diagnostics. Reconcile them separately at closeout. [Bolid operations](https://github.com/clickbytes/minimium/issues/587) · [Script operations](https://github.com/clickbytes/minimium/issues/593) · [Diagnostics](https://github.com/clickbytes/minimium/issues/595)

The client now registers `say` and `quit`, routes `dev inventory tools` to gameplay and passes quoted FOV values through the property validator. A canonical call, a property verb or a registered word command resolves through the host catalog. Client and administrative hosts expose the pure `ui.console.explain(text)` query.
