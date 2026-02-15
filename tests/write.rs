use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    path::Path,
    sync::Arc,
};

use serde_mosaic::*;

mod utilities;
use utilities::*;

#[test]
fn test_write_flat() {
    let name = "test_write_flat";

    let cup = Cup {
        name: name.to_string(),
        material: Material {
            id: 0,
            name: "ceramic".to_string(),
        },
    };

    let mut dbm = test_database();

    let _ = dbm.remove(&cup);

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Flat;
    write_options.name_collisions = NameCollisions::Overwrite;

    let (_, write_info) = dbm.write_verbose(&cup, &write_options).unwrap();
    assert_eq!(write_info.overwritten_files.len(), 0);
    assert_eq!(write_info.created_files.len(), 1);
    assert_eq!(
        write_info.created_files[0].file_stem().unwrap(),
        OsStr::new(name)
    );

    let _ = dbm.remove(&cup);
}

#[test]
fn test_write_link() {
    let cup = Cup {
        name: "daves_cup".to_string(),
        material: Material {
            id: 1,
            name: "ceramic".to_string(),
        },
    };

    let mut dbm = test_database();

    let _ = dbm.remove(&cup);

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.name_collisions = NameCollisions::Overwrite;

    let (_, write_info) = dbm.write_verbose(&cup, &write_options).unwrap();
    assert_eq!(write_info.overwritten_files.len(), 1);
    assert_eq!(write_info.created_files.len(), 1);
    assert_eq!(
        write_info.overwritten_files[0].file_stem().unwrap(),
        OsStr::new("ceramic")
    );
    assert_eq!(
        write_info.created_files[0].file_stem().unwrap(),
        OsStr::new("daves_cup")
    );

    let _ = dbm.remove(&cup);
}

#[test]
fn test_write_alias() {
    let mut dbm = test_database();

    // Cleanup before test
    let _ = dbm.remove((type_name::<Cup>(), "sarahs_cup"));
    let _ = dbm.remove((type_name::<Material>(), "china"));

    let cup = Cup {
        name: "aarons_cup".to_string(),
        material: Material {
            id: 2,
            name: "meissner".to_string(),
        },
    };

    let mut alias: HashMap<OsString, OsString> = HashMap::new();
    alias.insert(
        OsStr::new("aarons_cup").to_os_string(),
        OsStr::new("sarahs_cup").to_os_string(),
    );
    alias.insert(
        OsStr::new("meissner").to_os_string(),
        OsStr::new("china").to_os_string(),
    );

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.name_collisions = NameCollisions::Overwrite;
    write_options.alias = alias;

    let (_, write_info) = dbm.write_verbose(&cup, &write_options).unwrap();

    assert_eq!(write_info.created_files.len(), 2);
    assert_eq!(
        write_info.created_files[0].file_stem().unwrap(),
        OsStr::new("china")
    );
    assert_eq!(
        write_info.created_files[1].file_stem().unwrap(),
        OsStr::new("sarahs_cup")
    );

    // The original file names are not used in the database, but the aliases are
    assert!(!dbm.exists(&cup));
    assert!(!dbm.exists(&cup.material));
    assert!(dbm.exists((type_name::<Cup>(), "sarahs_cup")));
    assert!(dbm.exists((type_name::<Material>(), "china")));

    // Cleanup
    let _ = dbm.remove((type_name::<Cup>(), "sarahs_cup"));
    let _ = dbm.remove((type_name::<Material>(), "china"));
}

#[test]
fn test_write_wo_overwrite() {
    let material = Material {
        id: 3,
        name: "steel".to_string(),
    };

    let mut dbm = test_database();

    // Remove any leftover files from the last test
    let _ = dbm.remove((type_name::<Material>(), "steel_0"));
    let _ = dbm.remove((type_name::<Material>(), "steel_1"));
    let _ = dbm.remove((type_name::<Material>(), "steel_2"));

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.name_collisions = NameCollisions::AdjustName;

    // Stringify the written file and check the input
    let (file_path_0, write_info) = dbm.write_verbose(&material, &write_options).unwrap();

    // File is newly created
    assert_eq!(write_info.overwritten_files.len(), 0);
    assert_eq!(write_info.created_files.len(), 1);
    assert_eq!(
        write_info.created_files[0].file_name().unwrap(),
        OsStr::new("steel_0.yaml")
    );
    assert!(file_path_0.to_string_lossy().contains("steel_0"));

    let file_path_1 = dbm.write(&material, &write_options).unwrap();
    assert!(file_path_1.to_string_lossy().contains("steel_1"));

    let file_path_2 = dbm.write(&material, &write_options).unwrap();
    assert!(file_path_2.to_string_lossy().contains("steel_2"));

    assert!(file_path_0.exists());
    assert!(file_path_1.exists());
    assert!(file_path_2.exists());
    dbm.remove((type_name::<Material>(), "steel_0")).unwrap();
    dbm.remove((type_name::<Material>(), "steel_1")).unwrap();
    dbm.remove((type_name::<Material>(), "steel_2")).unwrap();

    // Check that the file does not exist
    assert!(!file_path_0.exists());
    assert!(!file_path_1.exists());
    assert!(!file_path_2.exists());
}

#[test]
fn test_to_be_removed() {
    let mut dbm = test_database();

    // Cleanup before test
    let _ = dbm.remove((type_name::<Cup>(), "to_be_removed"));
    let _ = dbm.remove((type_name::<Material>(), "to_be_removed"));

    assert!(!dbm.exists((type_name::<Cup>(), "to_be_removed")));
    assert!(!dbm.exists((type_name::<Material>(), "to_be_removed")));

    let wrapper = Cup {
        name: "to_be_removed".to_string(),
        material: Material {
            id: 0,
            name: "to_be_removed".to_string(),
        },
    };

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.name_collisions = NameCollisions::Overwrite;

    // Stringify the written file and check the input
    let _ = dbm.write(&wrapper, &write_options).unwrap();

    assert!(dbm.exists((type_name::<Cup>(), "to_be_removed")));
    assert!(dbm.exists((type_name::<Material>(), "to_be_removed")));

    dbm.remove_all("to_be_removed").unwrap();

    assert!(!dbm.exists((type_name::<Cup>(), "to_be_removed")));
    assert!(!dbm.exists((type_name::<Material>(), "to_be_removed")));
}

#[test]
fn test_write_arc() {
    let shovel = Shovel {
        name: "joes_shovel".into(),
        shaft: Arc::new(Material {
            id: 1,
            name: "special_wood".to_string(),
        }),
        blade: Material {
            id: 1,
            name: "special_steel".to_string(),
        },
    };

    let mut dbm = test_database();

    let path_shovel = format!(
        "{}/{}/{}.yaml",
        dbm.dir().to_string_lossy(),
        type_name::<Shovel>(),
        shovel.name().to_str().unwrap()
    );
    let path_shovel = Path::new(&path_shovel);

    let path_blade = format!(
        "{}/{}/{}.yaml",
        dbm.dir().to_string_lossy(),
        type_name::<Material>(),
        shovel.blade.name().to_str().unwrap()
    );
    let path_blade = Path::new(&path_blade);

    let path_shaft = format!(
        "{}/{}/{}.yaml",
        dbm.dir().to_string_lossy(),
        type_name::<Material>(),
        shovel.shaft.name().to_str().unwrap()
    );
    let path_shaft = Path::new(&path_shaft);

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.name_collisions = NameCollisions::Overwrite;

    // Stringify the written file and check the input
    dbm.write(&shovel, &write_options).unwrap();

    assert!(path_shovel.exists());
    assert!(path_blade.exists());
    assert!(path_shaft.exists());

    dbm.remove(&shovel).unwrap();
    dbm.remove(&shovel.blade).unwrap();
    dbm.remove(&*shovel.shaft).unwrap();

    // Check that the file does not exist
    assert!(!path_shovel.exists());
    assert!(!path_blade.exists());
    assert!(!path_shaft.exists());
}

#[test]
fn test_write_link_nested() {
    let mut dbm = test_database();

    let user = User {
        name: "Fred".into(),
        shovel: Arc::new(Shovel {
            name: "Freds_shovel".into(),
            shaft: Arc::new(Material {
                id: 4,
                name: "Freds_Birch".into(),
            }),
            blade: Material {
                id: 5,
                name: "Freds_Alloy".into(),
            },
        }),
    };

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.name_collisions = NameCollisions::Overwrite;

    dbm.write(&user, &write_options).unwrap();

    assert!(dbm.exists(&user));
    assert!(dbm.exists(&*user.shovel));
    assert!(dbm.exists(&*user.shovel.shaft));
    assert!(dbm.exists(&user.shovel.blade));

    dbm.remove(&user).unwrap();
    dbm.remove(&*user.shovel).unwrap();
    dbm.remove(&*user.shovel.shaft).unwrap();
    dbm.remove(&user.shovel.blade).unwrap();

    // Check that the file does not exist
    assert!(!dbm.exists(&user));
    assert!(!dbm.exists(&*user.shovel));
    assert!(!dbm.exists(&*user.shovel.shaft));
    assert!(!dbm.exists(&user.shovel.blade));
}

#[test]
fn write_opt() {
    let mut dbm = test_database();

    // With cup
    {
        let cupboard = Cupboard {
            name: "Ninas_cupboard".into(),
            cup: Some(Cup {
                name: "Ninas_cup".into(),
                material: Material {
                    id: 1,
                    name: "Ninas_ceramic".into(),
                },
            }),
        };

        let mut write_options = WriteOptions::default();
        write_options.write_mode = WriteMode::Link;
        write_options.name_collisions = NameCollisions::Overwrite;

        let (_, report) = dbm.write_verbose(&cupboard, &write_options).unwrap();

        assert!(dbm.exists(&cupboard));
        assert!(dbm.exists(&*cupboard.cup.as_ref().unwrap()));
        assert!(dbm.exists(&cupboard.cup.as_ref().unwrap().material));

        dbm.remove(&cupboard).unwrap();
        dbm.remove(&*cupboard.cup.as_ref().unwrap()).unwrap();
        dbm.remove(&cupboard.cup.as_ref().unwrap().material)
            .unwrap();

        assert!(!dbm.exists(&cupboard));
        assert!(!dbm.exists(&*cupboard.cup.as_ref().unwrap()));
        assert!(!dbm.exists(&cupboard.cup.as_ref().unwrap().material));

        assert_eq!(report.created_files.len(), 3);
    }

    // Without cup
    {
        let cupboard = Cupboard {
            name: "Ninas_cupboard".into(),
            cup: None,
        };

        let mut write_options = WriteOptions::default();
        write_options.write_mode = WriteMode::Link;
        write_options.name_collisions = NameCollisions::Overwrite;

        let (_, report) = dbm.write_verbose(&cupboard, &write_options).unwrap();

        assert!(dbm.exists(&cupboard));

        dbm.remove(&cupboard).unwrap();

        assert!(!dbm.exists(&cupboard));

        assert_eq!(report.created_files.len(), 1);
    }
}

#[test]
fn write_arc_opt() {
    let mut dbm = test_database();

    // With shovel
    {
        let shelf = Shelf {
            name: "Bens_shelf".into(),
            shovel: Some(Arc::new(Shovel {
                name: "Bens_shovel".into(),
                shaft: Arc::new(Material {
                    id: 4,
                    name: "Bens_birch".into(),
                }),
                blade: Material {
                    id: 5,
                    name: "Bens_alloy".into(),
                },
            })),
        };

        let mut write_options = WriteOptions::default();
        write_options.write_mode = WriteMode::Link;
        write_options.name_collisions = NameCollisions::Overwrite;

        let (_, report) = dbm.write_verbose(&shelf, &write_options).unwrap();

        assert!(dbm.exists(&shelf));
        assert!(dbm.exists(&**shelf.shovel.as_ref().unwrap()));
        assert!(dbm.exists(&*shelf.shovel.as_ref().unwrap().shaft));
        assert!(dbm.exists(&shelf.shovel.as_ref().unwrap().blade));

        dbm.remove(&shelf).unwrap();
        dbm.remove(&**shelf.shovel.as_ref().unwrap()).unwrap();
        dbm.remove(&*shelf.shovel.as_ref().unwrap().shaft).unwrap();
        dbm.remove(&shelf.shovel.as_ref().unwrap().blade).unwrap();

        // Check that the file does not exist
        assert!(!dbm.exists(&shelf));
        assert!(!dbm.exists(&**shelf.shovel.as_ref().unwrap()));
        assert!(!dbm.exists(&*shelf.shovel.as_ref().unwrap().shaft));
        assert!(!dbm.exists(&shelf.shovel.as_ref().unwrap().blade));

        assert_eq!(report.created_files.len(), 4);
    }

    // Without shovel
    {
        let shelf = Shelf {
            name: "Carls_shelf".into(),
            shovel: None,
        };

        let mut write_options = WriteOptions::default();
        write_options.write_mode = WriteMode::Link;
        write_options.name_collisions = NameCollisions::Overwrite;

        let (_, report) = dbm.write_verbose(&shelf, &write_options).unwrap();

        assert!(dbm.exists(&shelf));

        dbm.remove(&shelf).unwrap();

        // Check that the file does not exist
        assert!(!dbm.exists(&shelf));

        assert_eq!(report.created_files.len(), 1);
    }
}
