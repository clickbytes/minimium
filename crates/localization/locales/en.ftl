connection-connecting = Connecting to { $address }…
connection-disconnected = Disconnected
connection-starting-local = Starting or attaching to local server…
connection-awaiting-welcome = Awaiting server welcome
connection-not-connected = Not connected
connection-no-response = No server response yet
terminal-intro = Minimium client console. Type `help` for commands.
terminal-prompt = >
window-title = Minimium 3D client
window-create-failed = Window creation failed: { $detail }
connection-attempt-busy = Connection attempt { $id } is still finishing; retry shortly
connection-attempts-exhausted = Could not start connection: attempt identifiers exhausted
connection-worker-start-failed = Could not start connection worker: { $detail }
connection-timeout = Connection attempt timed out; waiting for cleanup before reconnecting
connection-waiting-cleanup = Waiting for cancelled connection cleanup
connection-setup-failed = Connection setup failed: { $detail }; you can reconnect
connection-failed = Connect failed: { $detail }; you can retry
connection-disconnect-or-wait = Disconnect or wait for the current connection attempt first
command-queued = Command queued
client-error = Client error: { $detail }
map-load-failed = Could not load map { $path }: { $detail }
bundled-map-invalid = Invalid bundled map: { $detail }
graphics-init-failed = Graphics initialization failed: { $detail }
renderer-already-initialized = Renderer can only initialize once
render-failed = Render error: { $detail }
fps-unavailable = FPS display is unavailable in the terminal client
console-history-help = F1 closes the console. Arrow keys browse command history.
shot-fired = Shot fired
visible-player-limit-exceeded = Server exceeded the visible-player snapshot limit
welcome-timeout = Server welcome timed out; you can reconnect
heartbeat-timeout = Server heartbeat timed out; you can reconnect
peer-disconnected = Server disconnected; you can reconnect
server-closed = Server closed the session; you can reconnect
connection-rejected = Server rejected connection: { $reason ->
    [protocol] incompatible protocol
    [busy] server is busy; this can include a duplicate active profile
    [full] server is full
    [rate_limit] request was rate-limited
   *[other] server rejected the request
}
server-error = Server error: { $reason ->
    [protocol] incompatible protocol
    [busy] server is busy
    [full] server is full
    [rate_limit] request was rate-limited
   *[other] request rejected
}
map-mismatch = Map mismatch: client={ $expected } server={ $actual }
connection-lost = Connection lost ({ $detail }); you can reconnect
connection-connected = Connected as player { $id } with { $health } HP
network-ping = Ping: { $milliseconds } ms
network-pong = PONG
server-accepted = Server accepted command
player-health = Health: { $health } HP
shot-missed = Shot missed
shot-hit = Hit player { $target } ({ $health } HP)
shot-target-respawned = Hit player { $target }; target respawned ({ $health } HP)
queue-failure = { $action ->
    [command] Could not queue command
    [fire] Could not fire
   *[movement] Could not queue movement
}: { $cause ->
    [disconnected] not connected
    [awaiting] awaiting welcome
    [authentication] authentication pending
    [stale] stale world context
    [full] outgoing queue is full
    [sequence] movement input sequence is exhausted; reconnect
   *[protocol] protocol rejected the message
}
window-title-status = Minimium | { $fps } FPS | { $health } HP | WASD move, Space jump, left click fire, Esc release
terminal-snapshot = server_time={ $time }ms full={ $full } self=({ $position }) velocity=({ $velocity }) grounded={ $grounded } player_updates={ $updates } player_removals={ $removals }
player-state = position=({ $position }) health={ $health } HP nearby_players={ $nearby }
network-stats = tx={ $sent } bytes rx={ $received } bytes rtt={ $ping }{ $nearby_active ->
    [yes]  nearby_players={ $nearby }
   *[no] { "" }
}
chat-line = [{ $name }#{ $id }] { $text }
altitude-result = { $result ->
    [success] set_altitude: destination reached
   *[rejected] set_altitude rejected: requires fly on, a planet and a valid height
}
world-transfer-prepared = World transfer prepared
world-transfer-committed = World transfer committed
world-transfer-cancelled = World transfer cancelled
items-available = { $count ->
    [one] { $count } item available
   *[other] { $count } items available
}
ui-status = { $status ->
    [earth_connecting] Connecting to Earth...
    [earth_disconnected] Earth: disconnected
    [earth_connected] Earth: connected
    [connected] Connected
    [earth_loading] Earth: loading terrain...
    [earth_choose_land] Earth: choose a point on land
    [earth_click_land] Earth: click land on the globe (Esc cancels)
    [earth_click_surface] Earth: click the globe surface
    [earth_moving] Earth: moving...
    [earth_moved] Earth: moved
    [terrain_materials] Preparing terrain materials
    [connection_timed_out] Connection timed out
    [earth_server_closed] Earth: server closed the connection
    [content_unavailable] World content unavailable
    [environment_initializing] Initializing
    [environment_synchronized] Synchronized
    [environment_stale] Stale
    [environment_unavailable] Unavailable
    [environment_local_demo] Local demo
   *[other] { $status }
}
earth-error = Earth error: { $detail }
ui-notice = { $notice ->
    [console_welcome] F1 opens the command console. Type help for commands.
    [console_overview] M toggles the world overview.
    [commands_header] Commands (PageUp/PageDown to scroll):
    [time_request_sent] Time change request sent
    [time_scale_request_sent] Time scale request sent
    [bolid_request_sent] Bolid request sent
    [bolid_request_queued] Bolid request queued until the world is ready
    [time_mode_request_sent] Time mode request sent
    [time_offline_policy_request_sent] Time offline policy request sent
    [sky_bodies_unavailable] Sky bodies are unavailable
    [map_teleport_desktop_only] map_telepo is available in the desktop Earth overview
    [sky_info_desktop_only] Sky info is unavailable; desktop renderer required
    [weather_requires_desktop] Regional weather diagnostics require the graphical client
    [meteors_require_desktop] Ambient meteor commands are available in the desktop client only
    [sky_quality_desktop_only] --sky-quality is available only in the desktop renderer
    [map_teleport_busy] map_telepo: wait for the previous destination
    [map_teleport_requires_earth] map_telepo requires a connected Earth world
    [local_server_threads_conflict] server.threads is specified both in client options and the host command
    [star_catalog_unavailable] Star catalog is unavailable; celestial labels are disabled
    [time_request_failed] Could not submit the time change request
    [time_scale_request_failed] Could not submit the time scale request
    [time_mode_request_failed] Could not submit the time mode request
    [time_offline_policy_request_failed] Could not submit the offline time policy request
    [time_scale_response_failed] The server did not apply the time scale request
    [environment_unavailable] Environment data is temporarily unavailable
    [clipboard_unavailable] Clipboard access is unavailable
    [altitude_requires_flight] set_altitude requires flight to be enabled
    [portal_snapshot_unavailable] Portal interaction requires a player snapshot
    [portal_unavailable] No nearby portal is available
    [local_server_config_failed] Could not prepare the local server configuration
   *[other] { $notice }
}
property-description = { $key ->
    [ui_locale] User interface language.
    [client_camera_fov] Vertical camera field of view.
    [client_input_mouse_sensitivity] Mouse look sensitivity; zero disables mouse rotation.
    [client_input_bindings] Physical key bindings by input context; conflicting controls in one context are rejected.
    [client_graphics_sky_quality] Local shadow and sky rendering quality.
    [client_sky_names] Show celestial labels in the desktop renderer.
    [client_sky_meteors_enabled] Enable client-local cosmetic meteors.
    [client_sky_meteors_rate] Session-only automatic meteor rate multiplier (0 to 10).
    [client_sky_meteors_quality] Requested meteor quality; limited by effective sky quality.
    [client_sky_meteors_seed_override] Session-only deterministic seed override: none or decimal u64.
    [client_sky_meteors_sound] Enable optional delayed meteor sound (desktop-audio build required).
    [client_audio_meteors] Enable optional delayed meteor audio in this session.
    [client_audio_meteor_volume] Meteor audio volume (0 to 1).
    [client_weather_cloud_detail] Preferred maximum recursive cloud voxel depth.
    [client_debug_atmosphere] Atmosphere diagnostic view.
    [client_debug_terrain] Terrain diagnostic view.
    [client_debug_clouds] Cloud diagnostic overlay.
    [client_sky_demo] Enable the local celestial demonstration.
    [player_movement_fly] Local fly/noclip input state; queue acceptance is not server confirmation.
    [player_movement_speed] Local walk/fly speed input; queue acceptance is not server confirmation.
    [player_movement_accel] Local walk/fly acceleration input; queue acceptance is not server confirmation.
    [world_time_mode] World clock mode; changes require the owned local server admin channel.
    [world_time_scale] World clock rate; changes require the owned local server admin channel.
    [world_time_offline_policy] Policy used when the world is offline.
    [world_time_utc] Synchronized world UTC date from the client replica.
    [world_time_revision] Clock revision from the client replica.
   *[other] { $key }
}
no-property-matches = No client properties match { $prefix }
command-help = { $name ->
    [terrain_status] terrain status
    [atmosphere_status] atmosphere status
    [atmosphere_debug] atmosphere debug off|transmission|scattering|optical_depth|distance|fog
    [terrain_debug] terrain debug off|albedo|normals|depth|shadow|base_color|coverage|habitat|source
    [weather_status] weather status
    [weather_clouds_debug] weather clouds debug off|regions|layers|wind|kinds|detail
    [weather_clouds_detail] weather clouds detail 2|3|4|5|6
    [set_time] set_time HHMM (UTC; owned local server)
    [set_time_local] set_time_local HHMM (local mean solar time at player; owned local server)
    [sky_quality] sky quality low|standard
    [sky_info] sky info
    [sky_bodies] sky bodies
    [sky_demo] sky demo on|off (local rendering demo)
    [camera_fov] camera fov [30..110] (vertical field of view in degrees; default 70)
    [sky_names] sky names true|false
    [time_status] time status
    [time_scale] time scale DECIMAL (alias timescale; owned local server; 0.1-1000000x or 0)
    [spawn_random_bolid] spawn_random_bolid (owned local server)
    [help] help
    [config_save] config save PATH
    [diagnostics_fps] diagnostics fps
    [network_stats] network stats
    [diagnostics_state] diagnostics state
    [console_clear] console clear
    [console_quit] console quit (alias: exit)
    [movement_move] movement move X Z (-1..1 each; tangent direction)
    [movement_jump] movement jump
    [world_portal] world portal
    [world_teleport] world teleport: click land on the Earth overview
    [combat_fire] combat fire
    [social_say] social say TEXT (alias: chat)
    [social_name] social name NAME
    [network_host_local] network host local [--threads 1|2|auto]
    [network_connect] network connect HOST:PORT
    [network_disconnect] network disconnect
    [network_ping] network ping
    [set_altitude] set_altitude <0..10000 km> - height above local ground; requires fly on
    [movement_fly] movement fly [on|off] - noclip; Space up, Ctrl down; inherits walk speed and accel
    [movement_speed] movement speed [0..10000] - maximum walk/fly speed in m/s; no argument shows current value
    [movement_accel] movement accel [0..10000] - walk/fly acceleration and braking in m/s²; no argument shows current value
   *[other] { $name }
}
command-rejected = { $code ->
    [unknown_command] Unknown command { $subject }.
    [invalid_arguments] Invalid arguments for { $subject }.
    [unknown_key] Unknown property or setting { $subject }.
    [invalid_value] Invalid value for { $subject }.
    [missing_value] A value is required for { $subject }.
    [duplicate_option] { $subject } was specified more than once.
    [conflicting_options] Conflicting options for { $subject }.
    [unknown_option] Unknown option { $subject }.
    [help_query] No help entry for { $subject }.
    [permission_denied] Permission denied for { $subject }.
    [unsupported_context] { $subject } is unavailable in this client context.
    [restart_required] { $subject } requires an application restart.
    [creation_only] { $subject } can only be set when creating the world.
    [read_only] { $subject } is read-only.
    [io] Could not complete the operation for { $subject }.
   *[other] The command could not be completed.
}
map-teleport-result = { $result ->
    [success] map_telepo: destination reached
   *[rejected] map_telepo: destination unavailable; choose land
}
content-unavailable = World content could not be prepared. Check the client log for details.
configuration-saved = Configuration saved to { $path }
configuration-save-failed = Could not save the configuration. Check the client log for details.
environment-status = Environment: { $quality } | { $date } | rate { $rate } | revision { $revision }
environment-title = { $quality } | { $date }
