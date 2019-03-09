use crate::prelude::*;
use crate::run::RunCommand;
use crate::utils::size::Size;
use std::env;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn is_running() -> bool {
    info!("checking if the docker daemon is running");
    RunCommand::new("docker")
        .args(&["info"])
        .hide_output(true)
        .run()
        .is_ok()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DockerEnv {
    image: String,
    local: bool,
}

impl DockerEnv {
    pub(crate) fn new(image: &str) -> Self {
        DockerEnv {
            image: image.to_string(),
            local: !image.contains('/'),
        }
    }

    pub(crate) fn ensure_exists_locally(&self) -> Fallible<()> {
        if !self.local {
            self.pull()?;
        } else {
            info!("docker environment is local, skipping pull");
        }

        info!("checking the image {} is available locally", self.image);
        RunCommand::new("docker")
            .args(&["image", "inspect", &self.image])
            .hide_output(true)
            .run()?;

        Ok(())
    }

    fn pull(&self) -> Fallible<()> {
        info!("pulling image {} from Docker Hub", self.image);
        RunCommand::new("docker")
            .args(&["pull", &self.image])
            .enable_timeout(false)
            .run()
    }

    #[cfg(windows)]
    fn build_number(&self) -> Fallible<crate::native::WindowsBuild> {
        RunCommand::new("docker")
            .args(&["run", &self.image, "powershell", "-Command", include_str!("./native/windows-build.ps1")])
            .run_capture()?
            .0
            .first()
            .unwrap()
            .parse()
            .map_err(Into::into)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum MountPerms {
    ReadWrite,
    ReadOnly,
}

struct MountConfig {
    host_path: PathBuf,
    container_path: PathBuf,
    perm: MountPerms,
}

impl MountConfig {
    fn to_volume_arg(&self) -> String {
        let perm = match self.perm {
            MountPerms::ReadWrite => "rw",
            MountPerms::ReadOnly => "ro",
        };
        format!(
            "{}:{}:{},Z",
            absolute(&self.host_path).to_string_lossy(),
            self.container_path.to_string_lossy(),
            perm
        )
    }

    fn to_mount_arg(&self) -> String {
        let mut opts_with_leading_comma = vec![];

        if self.perm == MountPerms::ReadOnly {
            opts_with_leading_comma.push(",readonly");
        }

        format!(
            "type=bind,src={},dst={}{}",
            absolute(&self.host_path).to_string_lossy(),
            self.container_path.to_string_lossy(),
            opts_with_leading_comma.join(""),
        )
    }
}

pub(crate) struct ContainerBuilder<'a> {
    image: &'a DockerEnv,
    mounts: Vec<MountConfig>,
    env: Vec<(String, String)>,
    memory_limit: Option<Size>,
    workdir: Option<String>,
    cmd: Vec<String>,
    enable_networking: bool,
}

impl<'a> ContainerBuilder<'a> {
    pub(crate) fn new(image: &'a DockerEnv) -> Self {
        ContainerBuilder {
            image,
            mounts: Vec::new(),
            env: Vec::new(),
            workdir: None,
            memory_limit: None,
            cmd: Vec::new(),
            enable_networking: true,
        }
    }

    pub(crate) fn mount<P1: Into<PathBuf>, P2: Into<PathBuf>>(
        mut self,
        host_path: P1,
        container_path: P2,
        perm: MountPerms,
    ) -> Self {
        self.mounts.push(MountConfig {
            host_path: host_path.into(),
            container_path: container_path.into(),
            perm,
        });
        self
    }

    pub(crate) fn env<S1: Into<String>, S2: Into<String>>(mut self, key: S1, value: S2) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub(crate) fn workdir<S: Into<String>>(mut self, workdir: S) -> Self {
        self.workdir = Some(workdir.into());
        self
    }

    pub(crate) fn memory_limit(mut self, limit: Option<Size>) -> Self {
        self.memory_limit = limit;
        self
    }

    pub(crate) fn cmd(mut self, cmd: Vec<String>) -> Self {
        self.cmd = cmd;
        self
    }

    pub(crate) fn enable_networking(mut self, enable: bool) -> Self {
        self.enable_networking = enable;
        self
    }

    pub(crate) fn create(self) -> Fallible<Container> {
        let mut args: Vec<String> = vec!["create".into()];

        for mount in &self.mounts {
            fs::create_dir_all(&mount.host_path)?;

            // On Windows, we mount paths containing a colon which don't work with `-v`, but on
            // Linux we need the Z flag, which doesn't work with `--mount`, for SELinux relabeling.
            if cfg!(windows) {
                args.push("--mount".into());
                args.push(mount.to_mount_arg())
            } else {
                args.push("-v".into());
                args.push(mount.to_volume_arg())
            }
        }

        for &(ref var, ref value) in &self.env {
            args.push("-e".into());
            args.push(format! {"{}={}", var, value})
        }

        if let Some(workdir) = self.workdir {
            args.push("-w".into());
            args.push(workdir);
        }

        if let Some(limit) = self.memory_limit {
            args.push("-m".into());
            args.push(limit.to_string());
        }

        if !self.enable_networking {
            args.push("--network".into());
            args.push("none".into());
        }

        args.push(self.image.image.clone());

        for arg in self.cmd {
            args.push(arg);
        }

        let (out, _) = RunCommand::new("docker").args(&*args).run_capture()?;
        Ok(Container { id: out[0].clone() })
    }

    pub(crate) fn run(self, quiet: bool) -> Fallible<()> {
        let container = self.create()?;

        // Ensure the container is properly deleted even if something panics
        scopeguard::defer! {{
            if let Err(err) = container.delete()
                .with_context(|_| format!("failed to delete container {}", container.id))
            {
                crate::utils::report_failure(&err);
            }
        }}

        container.run(quiet)?;
        Ok(())
    }
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        let cd = env::current_dir().expect("unable to get current dir");
        cd.join(path)
    }
}

#[derive(Debug, Fail)]
pub(crate) enum DockerError {
    #[fail(display = "container ran out of memory")]
    ContainerOOM,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectContainer {
    state: InspectState,
}

#[derive(Deserialize)]
struct InspectState {
    #[serde(rename = "OOMKilled")]
    oom_killed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Container {
    // Docker container ID
    id: String,
}

impl Display for Container {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.id.fmt(f)
    }
}

impl Container {
    fn inspect(&self) -> Fallible<InspectContainer> {
        let output = RunCommand::new("docker")
            .args(&["inspect", &self.id])
            .hide_output(true)
            .run_capture()?;

        let mut data: Vec<InspectContainer> = ::serde_json::from_str(&output.0.join("\n"))?;
        assert_eq!(data.len(), 1);
        Ok(data.pop().unwrap())
    }

    pub(crate) fn run(&self, quiet: bool) -> Fallible<()> {
        let res = RunCommand::new("docker")
            .args(&["start", "-a", &self.id])
            .quiet(quiet)
            .run();
        let details = self.inspect()?;

        // Return a different error if the container was killed due to an OOM
        if details.state.oom_killed {
            if let Err(err) = res {
                Err(err.context(DockerError::ContainerOOM).into())
            } else {
                Err(DockerError::ContainerOOM.into())
            }
        } else {
            res
        }
    }

    pub(crate) fn delete(&self) -> Fallible<()> {
        RunCommand::new("docker")
            .args(&["rm", "-f", &self.id])
            .run()
    }
}

#[cfg(windows)]
pub(crate) mod windows {
    use crate::native::{self, WindowsBuild};
    use crate::prelude::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use super::DockerEnv;

    /// The type of isolation a container can be run with.
    /// 
    /// Containers which can be run with `Process` isolation can also be run using `HyperV`.
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(crate) enum ContainerIsolation {
        HyperV,
        Process,
    }

    pub(crate) fn container_isolation_for_env(env: &DockerEnv) -> Result<ContainerIsolation, Arc<failure::Error>> {
        lazy_static! {
            static ref CACHE: Mutex<HashMap<DockerEnv, Result<ContainerIsolation, Arc<failure::Error>>>> = Default::default();
        }

        let mut cache = CACHE.lock().unwrap();
        cache.entry(env.clone()).or_insert_with(|| {
            let host = native::build_number();
            let guest: WindowsBuild = env.build_number().unwrap();

            container_isolation(host, guest)
                .map_err(Arc::new)
        }).clone()
    }

    fn container_isolation(host: WindowsBuild, guest: WindowsBuild) -> Fallible<ContainerIsolation> {
        use std::cmp::Ordering::*;
        use self::ContainerIsolation::*;

        if host.major != guest.major {
            bail!("Major version mismatch");
        }

        if host.minor != guest.minor {
            bail!("Minor version mismatch");
        }

        match host.build.cmp(&guest.build) {
            Less => bail!("Host build must be newer than container build"),
            Greater => Ok(HyperV),

            // For releases earlier than Windows 1709, guest must have the same revision number as host.
            Equal if host.revision == guest.revision || host.build >= 16299 => Ok(Process), 

            _ => bail!("Revision mismatch for Windows release <= 1709"),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_container_build() {
            let build = DockerEnv::new("mcr.microsoft.com/windows/servercore:10.0.16299.967")
                .build_number()
                .unwrap();

            assert_eq!(build, "10.0.16299.967".parse().unwrap());
        }

        #[test]
        fn test_container_isolation() {
            let win2016: WindowsBuild = "10.0.14393.953".parse().unwrap();
            let win1709: WindowsBuild = "10.0.16299.967".parse().unwrap();
            let win1803: WindowsBuild = "10.0.17134.590".parse().unwrap();

            assert_eq!(container_isolation(win1803, win1709).unwrap(), ContainerIsolation::HyperV);
            assert_eq!(container_isolation(win1803, win2016).unwrap(), ContainerIsolation::HyperV);

            assert_eq!(container_isolation(win1709, win1709).unwrap(), ContainerIsolation::Process);
            assert!(container_isolation(win2016, win1709).is_err());
        }

        #[test]
        fn test_revision_number_mismatch() {
            let host2016: WindowsBuild = "10.0.14393.196".parse().unwrap();
            let guest2016: WindowsBuild = "10.0.14393.257".parse().unwrap();

            assert!(container_isolation(host2016, guest2016).is_err());

            let host1803: WindowsBuild = "10.0.17134.196".parse().unwrap();
            let guest1803: WindowsBuild = "10.0.17134.257".parse().unwrap();

            assert_eq!(container_isolation(host1803, guest1803).unwrap(), ContainerIsolation::Process);
        }
    }
}
