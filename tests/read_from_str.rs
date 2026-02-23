use serde::Deserialize;
use serde_mosaic::*;

mod utilities;
use utilities::*;

#[test]
fn test_read_from_str() {
    #[derive(Deserialize)]
    struct Shelf {
        #[serde(deserialize_with = "deserialize_link")]
        shovel: Shovel,
    }

    let mut dbm = test_database();

    let shelf = indoc::indoc! {"
    ---
    shovel: 
      name: Georgs_shovel
    "};

    let shelf = dbm.from_str::<Shelf, SerdeYaml>(&shelf).unwrap();
    assert_eq!(shelf.shovel.name, "Georgs_shovel");
}

#[test]
fn test_read_from_str_opt() {
    #[derive(Deserialize)]
    struct Shelf {
        #[serde(deserialize_with = "deserialize_opt_link")]
        shovel: Option<Shovel>,
    }

    let mut dbm = test_database();

    let shelf = indoc::indoc! {"
    ---
    shovel:
    "};

    let shelf: Shelf = dbm.from_str::<Shelf, SerdeYaml>(&shelf).unwrap();
    assert!(shelf.shovel.is_none());

    let shelf = indoc::indoc! {"
    ---
    shovel: 
      name: Georgs_shovel
    "};

    let shelf: Shelf = dbm.from_str::<Shelf, SerdeYaml>(&shelf).unwrap();
    assert!(shelf.shovel.is_some());
    assert_eq!(shelf.shovel.unwrap().name, "Georgs_shovel");
}
