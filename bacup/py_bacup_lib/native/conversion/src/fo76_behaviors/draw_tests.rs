use super::*;

#[test]
fn unrelated_graphs_are_untouched() {
    for path in [
        "actors/character/behaviors/meleebehavior.hkx",
        "actors/character/behaviors/workbenchfurniturebehavior.hkx",
        "actors/moleminer/behaviors/gunbehavior.hkx",
        "actors/character/_1stperson/behaviors/pistol_gunwrappingbehavior.hkx",
    ] {
        let mut file = HkxFile::from_tagxml(11, VERSION, vec![]);
        assert_eq!(repair(&mut file, path).unwrap(), 0);
        assert!(file.objects().is_empty());
    }
}
