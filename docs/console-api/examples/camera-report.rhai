let path = "client.camera.fov";
let old = f1_get(path);
if !old.effective_available { throw "renderer_unavailable"; }
let changed = old.effective < 85.0;
let revision = old.revision;
if changed {
    let commit = f1_wait_committed(
        f1_set_if(path, 85.0, old.revision), 1000);
    revision = commit.property_revision;
}
let report = ();
let failed = false;
let failure = ();
try {
    report = f1_call("client.camera.status", []);
} catch (error) {
    failed = true;
    failure = error;
}
if changed {
    try {
        f1_wait_committed(f1_restore(old, revision), 1000);
    } catch (restore_error) {
        throw #{ code: "restore_failed", query_error: failure,
                 restore_error: restore_error };
    }
}
if failed { throw failure; }
report
