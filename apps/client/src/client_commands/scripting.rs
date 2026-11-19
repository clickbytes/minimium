#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScriptControl {
    RunFile(String),
    Status(u64),
    Cancel(u64),
}
