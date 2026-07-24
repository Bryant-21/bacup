use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use rayon::prelude::*;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn is_link_like(path: &Path, metadata: &fs::Metadata) -> io::Result<bool> {
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    let Some(parent) = path.parent() else {
        return Ok(false);
    };
    let Some(file_name) = path.file_name() else {
        return Ok(false);
    };
    Ok(fs::canonicalize(path)? != fs::canonicalize(parent)?.join(file_name))
}

fn remove_link(path: &Path) -> io::Result<()> {
    if fs::metadata(path).is_ok_and(|target| target.is_dir()) {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

fn remove_entry(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if is_link_like(path, &metadata)? {
        remove_link(path)
    } else if !metadata.is_dir() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    }
}

pub fn remove_path(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if is_link_like(path, &metadata)? {
        return remove_link(path);
    }

    let resolved = fs::canonicalize(path)?;
    if resolved.parent().is_none() || resolved.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to remove filesystem root: {}", resolved.display()),
        ));
    }
    if !metadata.is_dir() {
        return fs::remove_file(resolved);
    }

    let children = fs::read_dir(&resolved)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<PathBuf>>>()?;
    let worker_count = children.len().min(8);
    if worker_count <= 1 {
        for child in &children {
            remove_entry(child)?;
        }
    } else {
        rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .build()
            .map_err(io::Error::other)?
            .install(|| {
                children
                    .par_iter()
                    .try_for_each(|child| remove_entry(child))
            })?;
    }
    fs::remove_dir(resolved)
}

#[pyfunction(name = "conversion_remove_path")]
pub fn remove_path_py(py: Python<'_>, path: String) -> PyResult<()> {
    py.detach(move || {
        remove_path(Path::new(&path))
            .map_err(|error| PyRuntimeError::new_err(format!("failed to remove {path}: {error}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_top_level_children_in_parallel() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cleanup");
        for bucket in 0..4 {
            let directory = root.join(format!("bucket-{bucket}"));
            fs::create_dir_all(&directory).unwrap();
            for file in 0..8 {
                fs::write(directory.join(format!("file-{file}.bin")), b"data").unwrap();
            }
        }

        remove_path(&root).unwrap();

        assert!(!root.exists());
    }

    #[test]
    fn missing_tree_is_already_removed() {
        let temp = tempfile::tempdir().unwrap();
        remove_path(&temp.path().join("missing")).unwrap();
    }

    #[test]
    fn refuses_to_remove_filesystem_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().ancestors().last().unwrap();
        let error = remove_path(root).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
