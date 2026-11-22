//! Reader for `/sys/class/hwmon` temperature sensors.

use std::path::{Path, PathBuf};

use crate::types::Temperature;

/// Reads every temperature input exposed by the host's hwmon devices.
///
/// Missing directories and unreadable sensors are skipped: virtual machines
/// commonly expose no temperature sensors at all.
#[must_use]
pub(crate) fn read_all(hwmon_root: &Path) -> Vec<Temperature> {
    let Ok(entries) = std::fs::read_dir(hwmon_root) else {
        return Vec::new();
    };

    let mut chips: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    chips.sort();

    let mut readings = Vec::new();
    for chip_path in chips {
        let chip = read_trimmed(&chip_path.join("name")).unwrap_or_else(|| "hwmon".to_owned());
        readings.extend(read_chip(&chip_path, &chip));
    }
    readings.sort_by(|left, right| {
        left.chip
            .cmp(&right.chip)
            .then_with(|| left.label.cmp(&right.label))
    });
    readings
}

fn read_chip(chip_path: &Path, chip: &str) -> Vec<Temperature> {
    let Ok(entries) = std::fs::read_dir(chip_path) else {
        return Vec::new();
    };

    let mut inputs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_temperature_input(path))
        .collect();
    inputs.sort();

    inputs
        .iter()
        .filter_map(|input| read_sensor(input, chip))
        .collect()
}

fn is_temperature_input(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("temp") && name.ends_with("_input"))
}

fn read_sensor(input: &Path, chip: &str) -> Option<Temperature> {
    let millidegrees: f64 = read_trimmed(input)?.parse().ok()?;
    let label = input
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.trim_end_matches("_input").to_owned())?;
    let label = read_trimmed(&input.with_file_name(format!("{label}_label"))).unwrap_or(label);

    Some(Temperature {
        chip: chip.to_owned(),
        label,
        celsius: millidegrees_to_celsius(millidegrees),
    })
}

#[allow(clippy::cast_possible_truncation)]
fn millidegrees_to_celsius(millidegrees: f64) -> f32 {
    (millidegrees / 1000.0) as f32
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{millidegrees_to_celsius, read_all};

    fn scratch_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dandelion-sysmon-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn converts_millidegrees() {
        assert_eq!(millidegrees_to_celsius(42_500.0), 42.5);
        assert_eq!(millidegrees_to_celsius(-5_000.0), -5.0);
    }

    #[test]
    fn reads_labelled_sensors_and_skips_other_files() {
        let root = scratch_dir("hwmon");
        let chip = root.join("hwmon0");
        fs::create_dir_all(&chip).expect("create chip");
        fs::write(chip.join("name"), "coretemp\n").expect("write name");
        fs::write(chip.join("temp1_input"), "42500\n").expect("write temp");
        fs::write(chip.join("temp1_label"), "Package id 0\n").expect("write label");
        fs::write(chip.join("temp2_input"), "40000\n").expect("write temp");
        fs::write(chip.join("fan1_input"), "1200\n").expect("write fan");

        let readings = read_all(&root);

        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].chip, "coretemp");
        assert_eq!(readings[0].label, "Package id 0");
        assert_eq!(readings[0].celsius, 42.5);
        assert_eq!(readings[1].label, "temp2");

        fs::remove_dir_all(&root).expect("remove scratch dir");
    }

    #[test]
    #[allow(clippy::assert_is_empty)]
    fn missing_root_yields_no_readings() {
        assert!(read_all(&PathBuf::from("/definitely/not/a/hwmon")).is_empty());
    }
}
