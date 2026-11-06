# Setting the time of day

The desktop and text client accept `set_time HHMM` (UTC) and
`set_time_local HHMM` (local mean solar time at the player's longitude).
For example, `set_time_local 1333` sets 13:33 at the player's location.
Both commands preserve the current date in the selected time reference and
change the authoritative world clock for every player. Four digits are required;
hours must be 00–23 and minutes 00–59. Local solar time does not use civil time
zones or daylight-saving rules. The desktop local command requires an Earth
observer away from the poles. Commands require a server started by that client
and use its existing authenticated administration endpoint.
