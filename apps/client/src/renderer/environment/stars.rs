use minimium_assets::{
    embedded_sky_catalog, embedded_sky_catalog_identity, CatalogError, SkyCatalogIdentity,
    StarRecord,
};
use minimium_astronomy::StarAstrometry;

#[allow(dead_code)]
pub struct StarCatalogueAdapter {
    identity: SkyCatalogIdentity,
    records: &'static [StarRecord],
    astrometry: Box<[StarAstrometry]>,
}

#[allow(dead_code)]
impl StarCatalogueAdapter {
    pub fn load_embedded() -> Result<Self, CatalogError> {
        let catalogue = embedded_sky_catalog()?;
        let astrometry = catalogue
            .records()
            .iter()
            .map(|record| StarAstrometry {
                id: record.hyg_id,
                position_eqj_pc: record.position_eqj_pc,
                velocity_eqj_pc_per_jyear: record.velocity_eqj_pc_per_jyear,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            identity: embedded_sky_catalog_identity(),
            records: catalogue.records(),
            astrometry,
        })
    }

    pub fn identity(&self) -> SkyCatalogIdentity {
        self.identity
    }

    pub fn records(&self) -> &'static [StarRecord] {
        self.records
    }

    pub fn astrometry(&self) -> &[StarAstrometry] {
        &self.astrometry
    }
}
