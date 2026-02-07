///! Test of "normal" serialization and deserialization w/o database manager.
mod utilities;
use std::sync::Arc;

use utilities::*;

#[test]
fn test_serialize_and_deserialize() {
    fn inner() {
        let se_shovel = Shovel {
            name: "shovel".into(),
            shaft: Arc::new(Material {
                id: 1,
                name: "wood".to_string(),
            }),
            blade: Material {
                id: 1,
                name: "steel".to_string(),
            },
        };

        let text = serde_yaml::to_string(&se_shovel).unwrap();
        let expected = indoc::indoc! {"
        ---
        name: shovel
        shaft:
          id: 1
          name: wood
        blade:
          id: 1
          name: steel
        "
        };
        assert_eq!(text, expected);

        let de_shovel: Shovel = serde_yaml::from_str(&text).unwrap();
        assert_eq!(se_shovel, de_shovel);
    }

    // Without database manager in memory
    inner();

    // With database manager in memory (and therefore thread-local variables
    // available)
    let dbm = test_database();
    inner();
    let _ = dbm;
}

#[test]
fn test_serialize_and_deserialize_opt() {
    fn inner() {
        let se_cupboard = Cupboard {
            name: "cupboard".into(),
            cup: Some(Cup {
                name: "cup".into(),
                material: Material {
                    id: 1,
                    name: "ceramic".into(),
                },
            }),
        };

        let text = serde_yaml::to_string(&se_cupboard).unwrap();
        let expected = indoc::indoc! {"
        ---
        name: cupboard
        cup:
          name: cup
          material:
            id: 1
            name: ceramic
        "
        };
        assert_eq!(text, expected);

        let de_cupboard: Cupboard = serde_yaml::from_str(&text).unwrap();
        assert_eq!(se_cupboard, de_cupboard);
    }

    // Without database manager in memory
    inner();

    // With database manager in memory (and therefore thread-local variables
    // available)
    let dbm = test_database();
    inner();
    let _ = dbm;
}
