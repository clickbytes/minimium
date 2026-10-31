#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloudQuality {
    Sky,
    Low,
    Standard,
    High,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloudMethod {
    Analytic,
    Volumetric,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloudLighting {
    Low,
    Standard,
    High,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloudDebug {
    Off,
    Bounds,
    Ids,
    Density,
    Lod,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloudCommand {
    Status,
    Quality(CloudQuality),
    Scale(u8),
    Instances(u32),
    Lod(u32),
    Lighting(CloudLighting),
    Method(CloudMethod),
    Reset,
    Debug(CloudDebug),
}
