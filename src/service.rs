//! Root service providers and the `flankr service` command seam.

use std::ffi::OsString;
use std::path::PathBuf;
use flankr_daemon::{ServiceSpec, ServiceStatus, UserServiceBackend};

use crate::config::AppConfig;

/// Context shared by service providers.
#[derive(Debug, Clone)]
pub struct ServiceContext {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub executable: PathBuf,
    pub working_dir: PathBuf,
    pub socket: PathBuf,
    pub server_args: Vec<OsString>,
}

/// One named service exposed by flankr's registry.
pub trait ServiceProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn spec(&self, context: &ServiceContext) -> Result<ServiceSpec, String>;
    fn status(&self, context: &ServiceContext) -> Result<ServiceStatus, String>;
    fn start(&self, context: &ServiceContext) -> Result<ServiceStatus, String>;
    fn stop(&self, context: &ServiceContext) -> Result<ServiceStatus, String>;
    fn restart(&self, context: &ServiceContext) -> Result<ServiceStatus, String>;
}

fn backend() -> Result<Box<dyn UserServiceBackend>, String> {
    #[cfg(target_os = "macos")]
    {
        flankr_daemon::platform_backend()
            .map(|backend| Box::new(backend) as Box<dyn UserServiceBackend>)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(Box::new(flankr_daemon::platform_backend()))
    }
}

/// The flankr daemon itself as a persistent service.
#[derive(Debug, Default)]
pub struct FlankrProvider;

impl FlankrProvider {
    fn spec_for(&self, context: &ServiceContext) -> ServiceSpec {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let state = home.join(".local/state/flankr/services");
        let mut args = vec![OsString::from("server")];
        args.extend(context.server_args.iter().cloned());
        ServiceSpec {
            id: "dev.flankr.server".to_string(),
            program: context.executable.clone(),
            args,
            working_dir: context.working_dir.clone(),
            env: crate::daemon::passed_env(std::env::vars_os()),
            stdout: state.join("flankr.log"),
            stderr: state.join("flankr.log"),
            keep_alive: true,
        }
    }

    fn operation(&self, context: &ServiceContext, operation: Operation) -> Result<ServiceStatus, String> {
        let spec = self.spec_for(context);
        let backend = backend()?;
        let result = match operation {
            Operation::Status => backend.status(&spec),
            Operation::Start => backend.ensure(&spec),
            Operation::Stop => backend.stop(&spec),
            Operation::Restart => backend.restart(&spec),
        };
        result.map_err(|e| e.to_string())
    }
}

impl ServiceProvider for FlankrProvider {
    fn id(&self) -> &'static str {
        "flankr"
    }
    fn spec(&self, context: &ServiceContext) -> Result<ServiceSpec, String> {
        Ok(self.spec_for(context))
    }
    fn status(&self, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.operation(context, Operation::Status)
    }
    fn start(&self, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.operation(context, Operation::Start)
    }
    fn stop(&self, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.operation(context, Operation::Stop)
    }
    fn restart(&self, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.operation(context, Operation::Restart)
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Status,
    Start,
    Stop,
    Restart,
}

#[cfg(feature = "vpn-bitmot")]
#[derive(Debug, Default)]
pub struct BitmotProvider;

#[cfg(feature = "vpn-bitmot")]
impl BitmotProvider {
    fn vpn(&self, context: &ServiceContext) -> Result<crate::server::link::vpn::bitmot::Bitmot, String> {
        let endpoint = context
            .config
            .link
            .bitmot_endpoint
            .as_deref()
            .filter(|endpoint| !endpoint.trim().is_empty())
            .ok_or_else(|| "vpn.bitmot requires [link] bitmot_endpoint".to_string())?;
        Ok(crate::server::link::vpn::bitmot::Bitmot::find(endpoint))
    }

    fn op(&self, context: &ServiceContext, operation: Operation) -> Result<ServiceStatus, String> {
        let vpn = self.vpn(context)?;
        let port = context
            .config
            .link
            .listen
            .as_deref()
            .and_then(|list| list.first())
            .and_then(|value| value.rsplit_once(':'))
            .and_then(|(_, port)| port.parse().ok())
            .unwrap_or(crate::server::link::DEFAULT_PORT);
        if matches!(operation, Operation::Start | Operation::Restart) {
            let address: SocketAddr = format!("127.0.0.1:{port}")
                .parse()
                .map_err(|e| format!("vpn.bitmot local target is invalid: {e}"))?;
            if TcpStream::connect_timeout(&address, std::time::Duration::from_millis(500)).is_err() {
                return Err(format!(
                    "vpn.bitmot local link target 127.0.0.1:{port} is not listening; start flankr first"
                ));
            }
        }
        let spec = vpn.service_spec(port);
        let backend = backend()?;
        let result = match operation {
            Operation::Status => backend.status(&spec),
            Operation::Start => backend.ensure(&spec),
            Operation::Stop => backend.stop(&spec),
            Operation::Restart => backend.restart(&spec),
        };
        result.map_err(|e| e.to_string())
    }
}

#[cfg(feature = "vpn-bitmot")]
impl ServiceProvider for BitmotProvider {
    fn id(&self) -> &'static str {
        "vpn.bitmot"
    }
    fn spec(&self, context: &ServiceContext) -> Result<ServiceSpec, String> {
        let vpn = self.vpn(context)?;
        Ok(vpn.service_spec(crate::server::link::DEFAULT_PORT))
    }
    fn status(&self, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.op(context, Operation::Status)
    }
    fn start(&self, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.op(context, Operation::Start)
    }
    fn stop(&self, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.op(context, Operation::Stop)
    }
    fn restart(&self, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.op(context, Operation::Restart)
    }
}

/// Registered providers for the current configuration.
pub struct Registry {
    flankr: FlankrProvider,
    #[cfg(feature = "vpn-bitmot")]
    bitmot: Option<BitmotProvider>,
}

impl Registry {
    pub fn new(context: &ServiceContext) -> Self {
        Self {
            flankr: FlankrProvider,
            #[cfg(feature = "vpn-bitmot")]
            bitmot: (context.config.link.vpn.as_deref() == Some("bitmot")
                || context.config.link.bitmot_endpoint.is_some())
                .then_some(BitmotProvider),
        }
    }

    fn provider(&self, id: &str) -> Option<&dyn ServiceProvider> {
        match id {
            "flankr" => Some(&self.flankr),
            #[cfg(feature = "vpn-bitmot")]
            "vpn.bitmot" if self.bitmot.is_some() => self.bitmot.as_ref().map(|p| p as &dyn ServiceProvider),
            _ => None,
        }
    }

    pub fn ids(&self) -> Vec<&'static str> {
        let mut ids = vec!["flankr"];
        #[cfg(feature = "vpn-bitmot")]
        if self.bitmot.is_some() {
            ids.push("vpn.bitmot");
        }
        ids
    }

    pub fn status(&self, id: &str, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.provider(id)
            .ok_or_else(|| format!("unknown service '{id}'; use flankr or vpn.bitmot"))?
            .status(context)
    }

    pub fn start(&self, id: &str, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.provider(id)
            .ok_or_else(|| format!("unknown service '{id}'; use flankr or vpn.bitmot"))?
            .start(context)
    }

    pub fn stop(&self, id: &str, context: &ServiceContext) -> Result<ServiceStatus, String> {
        let provider = self
            .provider(id)
            .ok_or_else(|| format!("unknown service '{id}'; use flankr or vpn.bitmot"))?;
        let status = provider.stop(context)?;
        // Stopping the daemon is the explicit dependency cleanup operation;
        // restart intentionally leaves the Bitmot relay untouched.
        if id == "flankr" {
            #[cfg(feature = "vpn-bitmot")]
            if self.bitmot.is_some() {
                let _ = self.bitmot.as_ref().expect("checked above").stop(context);
            }
        }
        Ok(status)
    }

    pub fn restart(&self, id: &str, context: &ServiceContext) -> Result<ServiceStatus, String> {
        self.provider(id)
            .ok_or_else(|| format!("unknown service '{id}'; use flankr or vpn.bitmot"))?
            .restart(context)
    }
}

/// Dispatch a service operation and print one stable status line per service.
pub fn command(operation: &str, service: Option<&str>, context: &ServiceContext) -> Result<i32, String> {
    let registry = Registry::new(context);
    match operation {
        "list" => {
            for id in registry.ids() {
                let status = registry.status(id, context)?;
                println!("{id}: {}", status.description());
            }
            Ok(0)
        }
        "status" | "start" | "stop" | "restart" => {
            let id = service.ok_or_else(|| format!("flankr service {operation} requires a service name"))?;
            let status = match operation {
                "status" => registry.status(id, context)?,
                "start" => registry.start(id, context)?,
                "stop" => registry.stop(id, context)?,
                _ => registry.restart(id, context)?,
            };
            println!("{id}: {}", status.description());
            Ok(0)
        }
        _ => Err("usage: flankr service list|status|start|stop|restart [service]".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_always_contains_flankr() {
        let context = ServiceContext {
            config: AppConfig::default(),
            config_path: PathBuf::from("/tmp/config.toml"),
            executable: PathBuf::from("/bin/flankr"),
            working_dir: PathBuf::from("/tmp"),
            socket: PathBuf::from("/tmp/flankr.sock"),
            server_args: Vec::new(),
        };
        assert!(Registry::new(&context).ids().contains(&"flankr"));
    }
}
