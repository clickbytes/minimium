use super::*;

pub(crate) fn diff_remote_players(
    previous: &RemotePlayers,
    current: &RemotePlayers,
) -> (RemotePlayers, RemotePlayerIds) {
    let mut changed = RemotePlayers::default();
    for player in current.iter() {
        let differs = previous
            .get(player.id)
            .is_none_or(|old| old.name != player.name || old.position_mm != player.position_mm);
        if differs {
            let _ = changed.push(player.clone());
        }
    }

    let mut removed = RemotePlayerIds::default();
    for previous_player in previous.iter() {
        if current.get(previous_player.id).is_none() {
            let _ = removed.push(previous_player.id);
        }
    }
    (changed, removed)
}
