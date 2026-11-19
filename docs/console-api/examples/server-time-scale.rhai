let request = f1_set("world.time.scale", scale("1000"));
let committed = f1_wait_committed(request, 5000);
let sample = f1_call("world.time.status", []);
#{ request: request, commit: committed, session_sample: sample }
