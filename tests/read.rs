/*!
All database entries belonging to this test have a prepending 02.
 */

use serde_mosaic::*;
use serde_yaml::Value;
use std::{ptr, sync::Arc};

mod utilities;
use utilities::*;

#[test]
fn test_read_flat() {
    let mut dbm = test_database();
    let cup: Cup = dbm.read("mikes_cup").unwrap();
    assert_eq!(cup.material.id, 2);
}

#[test]
fn test_read_link() {
    let mut dbm = test_database();

    let cup: Cup = dbm.read("joes_cup").unwrap();
    assert_eq!(cup.material.id, 2);
}

#[test]
fn test_read_link_err() {
    let mut dbm = test_database();

    let err = dbm.read::<Cup, _>("bad_file").unwrap_err();
    let err_msg = err.to_string();
    assert!(err_msg.contains("invalid type: string \"42.0\", expected usize"));
}

#[test]
fn test_read_arc_link() {
    let mut dbm = test_database();
    let shovel: Shovel = dbm.read("shovel").unwrap();
    assert_eq!(shovel.shaft.id, 3);
    assert_eq!(shovel.blade.id, 2);
}

/**
Creates multiple "Shovel" instances which share the same "Material" instance for
their shaft.
 */
#[test]
fn test_read_arc_link_reuse() {
    let mut dbm = test_database();

    // Write a test file to make sure that the checksum is up-to-date
    let shovel = Shovel {
        name: "franks_shovel".into(),
        shaft: Arc::new(Material {
            id: 10,
            name: "spruce".to_string(),
        }),
        blade: Material {
            id: 11,
            name: "brass".to_string(),
        },
    };

    // Cleanup
    dbm.remove(&shovel).unwrap();
    dbm.remove(&shovel.blade).unwrap();
    dbm.remove(&*shovel.shaft).unwrap();

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.name_collisions = NameCollisions::Overwrite;

    dbm.write(&shovel, &write_options).unwrap();

    assert_eq!(dbm.cache().len(), 0);
    let shovel_1: Shovel = dbm.read(shovel.name()).unwrap();
    assert_eq!(dbm.cache().len(), 1);
    let shovel_2: Shovel = dbm.read(shovel.name()).unwrap();

    // These two instances share the same pointer
    assert!(ptr::eq(&*shovel_1.shaft, &*shovel_2.shaft));

    // Manipulate the checksum of the file so the cached arc does not get reused
    // anymore.
    {
        let file_path = dbm.full_path(&shovel).expect("exists");
        let contents = std::fs::read_to_string(&file_path).expect("readable");
        let mut file: Value = serde_yaml::from_str(&contents).expect("valid yaml");
        let old_val = file["Shovel"]["shaft"]["checksum"]
            .as_i64()
            .expect("is integer");
        file["Shovel"]["shaft"]["checksum"] = Value::from(old_val + 1);
        let updated = serde_yaml::to_string(&file).unwrap();
        std::fs::write(&file_path, updated).expect("writable");
    }

    let shovel_3: Shovel = dbm.read(shovel.name()).unwrap();

    // No pointer sharing, since the checksum was not identical
    assert!(!ptr::eq(&*shovel_1.shaft, &*shovel_3.shaft));

    // Cleanup
    dbm.remove(&shovel).unwrap();
    dbm.remove(&shovel.blade).unwrap();
    dbm.remove(&*shovel.shaft).unwrap();
}

#[test]
fn test_read_nested() {
    let mut dbm = test_database();

    let user: User = dbm.read("mike").unwrap();
    assert_eq!(user.shovel.blade.id, 2);
    assert_eq!(user.shovel.shaft.id, 3);
}

/**
Test reading two fields which link to the same database field. The resulting Arc
pointers should point to the same memory location, since the fields should be aliasing.
 */
#[test]
fn test_reuse_inside_same_struct() {
    let mut dbm = test_database();

    let stool: Stool = dbm.read("stool").unwrap();

    // Check that the pointers of fields 1-3 are aliasing
    assert!(ptr::eq(&*stool.leg_1, &*stool.leg_2));
    assert!(ptr::eq(&*stool.leg_1, &*stool.leg_3));
    assert!(ptr::eq(&*stool.leg_2, &*stool.leg_3));

    // But field 4 is not aliasing with the other three!
    assert!(!ptr::eq(&*stool.leg_1, &*stool.seat));
    assert!(!ptr::eq(&*stool.leg_2, &*stool.seat));
    assert!(!ptr::eq(&*stool.leg_3, &*stool.seat));
}

#[test]
fn test_read_opt() {
    let mut dbm = test_database();

    let full_cupboard: Cupboard = dbm.read("full_cupboard").unwrap();
    assert_eq!(
        full_cupboard.cup,
        Some(Cup {
            name: "joes_cup".into(),
            material: Material {
                id: 2,
                name: "steel".into()
            }
        })
    );

    let empty_cupboard: Cupboard = dbm.read("empty_cupboard").unwrap();
    assert_eq!(empty_cupboard.cup, None);
}

#[test]
fn test_read_arc_opt() {
    let mut dbm = test_database();

    let full_shelf: Shelf = dbm.read("full_shelf").unwrap();
    assert!(full_shelf.shovel.is_some());

    let empty_shelf: Shelf = dbm.read("empty_shelf").unwrap();
    assert!(empty_shelf.shovel.is_none());
}
