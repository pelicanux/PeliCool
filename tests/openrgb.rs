use pelicool::openrgb::{hex_color, parse_motherboard, profile_board};
#[test]
fn saved_profile_selects_only_motherboard_and_decodes_bgr_storage() {
    let raw=br#"{"controllers":[{"type":1,"name":"RAM"},{"type":0,"name":"ASRock Test","active_mode":0,"modes":[{"name":"Direct"}],"colors":[1024,16128,197121]}]}"#;
    let board = profile_board(raw).unwrap();
    assert_eq!(board.name, "ASRock Test");
    assert_eq!(board.mode, "Direct");
    assert_eq!(hex_color(board.colors[0]), "000400");
    assert_eq!(hex_color(board.colors[1]), "003F00");
    assert_eq!(hex_color(board.colors[2]), "010203");
}
#[test]
fn offline_path_rejects_effects_and_ambiguous_boards() {
    for raw in [br#"{"controllers":[{"type":0},{"type":0}]}"#.as_slice(),br#"{"controllers":[{"type":0,"name":"Board","active_mode":0,"modes":[{"name":"Rainbow"}],"colors":[1]}]}"#]{assert!(profile_board(raw).is_err());}
}
fn string(raw: &mut Vec<u8>, value: &str) {
    raw.extend(((value.len() + 1) as u16).to_le_bytes());
    raw.extend(value.as_bytes());
    raw.push(0);
}
#[test]
fn sdk_parser_preserves_mode_and_colors_and_rejects_truncation() {
    let mut raw = vec![0; 8];
    string(&mut raw, "Board");
    for _ in 0..4 {
        string(&mut raw, "");
    }
    raw.extend(1u16.to_le_bytes());
    raw.extend(0u32.to_le_bytes());
    string(&mut raw, "Direct");
    raw.extend([0u8; 36]);
    raw.extend(0u16.to_le_bytes());
    raw.extend(0u16.to_le_bytes());
    raw.extend(0u16.to_le_bytes());
    raw.extend(1u16.to_le_bytes());
    raw.extend([1, 2, 3, 0]);
    let len = raw.len() as u32;
    raw[..4].copy_from_slice(&len.to_le_bytes());
    let board = parse_motherboard(&raw).unwrap().unwrap();
    assert_eq!(board.name, "Board");
    assert_eq!(board.colors, [10, 0, 0, 0, 1, 0, 1, 2, 3, 0]);
    assert_eq!(&board.mode[4..8], &[0; 4]);
    for len in 0..raw.len() {
        assert!(parse_motherboard(&raw[..len]).is_err());
    }
}
