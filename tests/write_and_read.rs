use std::sync::Arc;

use serde_mosaic::*;

mod utilities;
use utilities::*;

#[test]
fn write_and_read_arc() {
    let mut dbm = test_database();

    let w_shelf = Shelf {
        name: "Georgs_shelf".into(),
        shovel: Some(Arc::new(Shovel {
            name: "Georgs_shovel".into(),
            shaft: Arc::new(Material {
                id: 4,
                name: "Georgs_birch".into(),
            }),
            blade: Material {
                id: 5,
                name: "Georgs_alloy".into(),
            },
        })),
    };

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.name_collisions = NameCollisions::Overwrite;

    dbm.write(&w_shelf, &write_options).unwrap();

    let r_shelf: Shelf = dbm.read(w_shelf.name()).unwrap();
    assert_eq!(w_shelf, r_shelf);
}
