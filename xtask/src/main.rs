// Some of this was taken from the "opte" project by the Oxide Computer Company.

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, ValueEnum};
use cargo_metadata::Metadata;
use std::process::Command;
use std::sync::OnceLock;
use fs_extra;

static METADATA: OnceLock<Metadata> = OnceLock::new();
fn cargo_meta() -> &'static Metadata {
    METADATA
        .get_or_init(|| cargo_metadata::MetadataCommand::new().exec().unwrap())
}

#[derive(Debug, Parser)]
enum Xtask {
    /// Build the app.
    Build(BuildOptions),

    /// Run the app -- similar to cargo run.
    Run(RunOptions),

    /// Build the Docker container for open-epaper-gen. 
    Package,
}

#[derive(Debug, Args)]
struct RunOptions {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Debug, Args)]
struct BuildOptions {
    #[arg(long)]
    platform: BuildPlatform,

    #[arg(long)]
    release: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum BuildPlatform {
    Native,
    LinuxX64,
}

fn main() -> anyhow::Result<()> {
    let cmd = Xtask::parse();

    match cmd {
        Xtask::Build(options) => cmd_build(options),
        Xtask::Run(options) => cmd_run(options),
        Xtask::Package => cmd_package(),
    }
}

fn build_cargo_bin(
    target: &[&str],
    release: bool,
    cwd: Option<&str>,
    current_cargo: bool,
) -> Result<()> {
    let meta = cargo_meta();

    let mut dir = meta.workspace_root.clone();
    if let Some(cwd) = cwd {
        dir.push(cwd);
    }

    let mut command = if current_cargo {
        let cargo =
            std::env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
        Command::new(cargo)
    } else {
        Command::new("cargo")
    };

    command.arg("build");
    command.args(target);
    if release {
        command.arg("--release");
    }

    let mut dir = meta.workspace_root.clone().into_std_path_buf();
    if let Some(cwd) = cwd {
        dir.push(cwd);
    }

    command.current_dir(dir);

    command.output_nocapture().context(format!(
        "failed to build {:?}",
        if target.is_empty() {
            cwd.unwrap_or("<unnamed>")
        } else {
            target[target.len() - 1]
        }
    ))
}

fn build_cross_bin(
    target: &[&str],
    release: bool,
    cwd: Option<&str>,
    platform: BuildPlatform,
) -> Result<()> {
    let meta = cargo_meta();

    let mut dir = meta.workspace_root.clone();
    if let Some(cwd) = cwd {
        dir.push(cwd);
    }

    let mut command = Command::new("cross");
    command.arg("build");
    command.args(target);
    if release {
        command.arg("--release");
    }
    let cross_platform = match platform {
        BuildPlatform::Native =>
            anyhow::bail!("Cannot use cross to build on native platform"),
        BuildPlatform::LinuxX64 => "x86_64-unknown-linux-gnu",
    };
    command.arg(format!("--target={}", cross_platform));

    let mut dir = meta.workspace_root.clone().into_std_path_buf();
    if let Some(cwd) = cwd {
        dir.push(cwd);
    }

    command.current_dir(dir);

    command.output_nocapture().context(format!(
        "failed to build {:?} using cross -- do you have cross set up?",
        if target.is_empty() {
            cwd.unwrap_or("<unnamed>")
        } else {
            target[target.len() - 1]
        }
    ))
}

fn copy_resources_to_output(release: bool) -> Result<()> {
    let meta = cargo_meta();
    let resources_dir = meta
        .workspace_root
        .join("open-epaper-gen")
        .join("resources");

    let target_dir = meta
        .target_directory
        .join(if release { "release" } else { "debug" });

    let mut options = fs_extra::dir::CopyOptions::new();
    options.overwrite = true;

    Ok(fs_extra::copy_items(&[resources_dir], target_dir, &options).map(|_| ())?)
}

fn cmd_build(options: BuildOptions) -> Result<()> {
    let mode = if options.release { "release" } else { "debug" };
    println!(
        "Building open-epaper-gen for {:?} in {} configuration...",
        options.platform,
        mode
    );
    if options.platform == BuildPlatform::Native {
        build_cargo_bin(&[], options.release, None, true)?;
    } else {
        build_cross_bin(&[], options.release, None, options.platform)?;
    }
    copy_resources_to_output(options.release)
}

fn cmd_run(options: RunOptions) -> Result<()> {
    build_cargo_bin(&[], false, None, true)?;
    copy_resources_to_output(false)?;
    
    let meta = cargo_meta();
    let target_bin = meta
        .target_directory
        .join("debug")
        .join("open-epaper-gen");

    let source_dir = meta
        .workspace_root
        .join("open-epaper-gen");

    let mut command = Command::new(target_bin);
    command.args(options.args);
    command.current_dir(source_dir);

    command.output_nocapture()
}

fn docker_tags() -> Result<Vec<String>> {
    let meta = cargo_meta();
    let package = meta.packages.iter()
        .find(|p| p.name == "open-epaper-gen")
        .ok_or(anyhow!("Could not find open-epaper-gen in workspace packages."))?;

    Ok(vec![
        format!(
            "{}.{}.{}",
            package.version.major,
            package.version.minor,
            package.version.patch
        ),
        String::from("latest"),
    ])
}

fn cmd_package() -> Result<()> {
    cmd_build(BuildOptions{
        platform: BuildPlatform::LinuxX64,
        release: true,
    })?;

    println!("Building Docker container for open-epaper-gen...");

    let meta = cargo_meta();

    let mut command = Command::new("docker");
    command.args(["build", "."]);

    for tag in docker_tags()? {
        command.arg(format!("--tag=open-epaper-gen:{}", tag));
    }

    command.current_dir(meta.workspace_root.clone());

    command.output_nocapture()
}

trait CommandNoCapture {
    fn output_nocapture(&mut self) -> Result<()>;
}

impl CommandNoCapture for Command {
    fn output_nocapture(&mut self) -> Result<()> {
        let status = self
            .spawn()
            .context("failed to spawn child cargo invocation")?
            .wait()
            .context("failed to await child cargo invocation")?;
        
        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("failed to run (status {status})")
        }
    }
}
