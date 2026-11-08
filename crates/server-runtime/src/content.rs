//! Runtime composition of the optional asset-I/O material endpoint.
use std::io;

pub(crate) struct ContentServer {
    pub hash: [u8; 32],
    pub port: u16,
    _server: Option<minimium_asset_io::ContentServer>,
}

impl ContentServer {
    pub(crate) fn shutdown(&mut self, timeout: std::time::Duration) -> Vec<String> {
        self._server
            .as_mut()
            .map_or_else(Vec::new, |server| server.shutdown(timeout))
    }
}

impl ContentServer {
    pub(crate) fn new(
        config: &crate::config::ServerConfig,
        logger: minimium_diagnostics::DomainLogger,
    ) -> io::Result<Self> {
        let Some(root) = config.material_package_path.as_deref() else {
            return Ok(Self {
                hash: [0; 32],
                port: 0,
                _server: None,
            });
        };
        let server = minimium_asset_io::ContentServer::start(
            root,
            &config.bind_address,
            config.material_http_port,
            logger,
        )?;
        Ok(Self {
            hash: server.hash,
            port: server.port,
            _server: Some(server),
        })
    }
}
