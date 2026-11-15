//! Package a compiled client and an explicit manifest without interpreting recipes.
use minimium_asset_io::materials::{read_bounded, MaterialPackage};
use minimium_assets::terrain_material::{MaterialManifest, MAX_MANIFEST_BYTES};
use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [executable, manifest, output] = args.as_slice() else {
        return Err("usage: package_client EXECUTABLE MANIFEST NEW_OUTPUT_DIRECTORY".into());
    };
    let executable = Path::new(executable);
    let manifest = Path::new(manifest);
    let output = Path::new(output);
    if output.try_exists()? {
        return Err("package destination already exists; choose a new directory".into());
    }
    let source = read_bounded(manifest, MAX_MANIFEST_BYTES as u64)?;
    let descriptor = MaterialManifest::parse(&source)?;
    let source_directory = manifest.parent().unwrap_or_else(|| Path::new("."));
    let mut staging = Staging::new(output)?;
    let materials = staging.directory.join("materials");
    fs::create_dir(&materials)?;
    staging.directories.push(materials.clone());
    for compressed in [true, false] {
        let variant = usize::from(!compressed);
        let mut maps = [Vec::new(), Vec::new(), Vec::new()];
        for (bytes, file) in maps.iter_mut().zip(&descriptor.variants[variant].maps) {
            *bytes = read_bounded(&source_directory.join(&file.name), file.bytes)?;
        }
        // Validate and copy the same buffers, even if the source changes during packaging.
        let package = MaterialPackage::from_parts(&source, compressed, maps)?;
        for (bytes, file) in package.maps.iter().zip(&descriptor.variants[variant].maps) {
            staging.write(&materials.join(&file.name), bytes)?;
        }
    }
    staging.write(&materials.join("manifest.ron"), &source)?;
    let filename = executable.file_name().ok_or("executable has no filename")?;
    let target = staging.directory.join(filename);
    let mut input = fs::File::open(executable)?;
    if !input.metadata()?.is_file() {
        return Err("executable must be a regular file".into());
    }
    let mut destination = staging.create_file(&target)?;
    io::copy(&mut input, &mut destination)?;
    fs::set_permissions(&target, input.metadata()?.permissions())?;
    drop(destination);
    fs::rename(&staging.directory, output)?;
    staging.published = true;
    println!("{}", output.display());
    Ok(())
}

struct Staging {
    directory: PathBuf,
    files: Vec<PathBuf>,
    directories: Vec<PathBuf>,
    published: bool,
}

impl Staging {
    fn new(output: &Path) -> io::Result<Self> {
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        for _ in 0..8 {
            let random = minimium_platform::endpoint::new_random_bytes::<16>()?;
            let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let directory = parent.join(format!(".minimium-package-{suffix}"));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    return Ok(Self {
                        directories: vec![directory.clone()],
                        directory,
                        files: Vec::new(),
                        published: false,
                    })
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "package staging collision",
        ))
    }

    fn create_file(&mut self, path: &Path) -> io::Result<fs::File> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        self.files.push(path.to_owned());
        Ok(file)
    }

    fn write(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.create_file(path)?.write_all(bytes)
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if self.published {
            return;
        }
        // Only paths created by this invocation are removed; no recursive traversal.
        for path in self.files.iter().rev() {
            if let Err(error) = fs::remove_file(path) {
                eprintln!("package cleanup {}: {error}", path.display());
            }
        }
        for path in self.directories.iter().rev() {
            if let Err(error) = fs::remove_dir(path) {
                eprintln!("package cleanup {}: {error}", path.display());
            }
        }
    }
}
